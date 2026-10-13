// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Printing on Linux: the file's PDF is handed to the desktop's own print
//! dialog, through the XDG desktop portal's Print interface (the GTK, GNOME
//! and KDE portals have it), which sends it to the printer chosen there.
//!
//! Where no portal prints (a window manager's portal alone), the PDF is
//! opened in the system's PDF viewer instead, to print from there: the same
//! way out as macOS' when Preview's print dialog cannot be opened.

use std::path::Path;

use ashpd::desktop::ResponseError;
use ashpd::desktop::print::{PageSetup, PreparePrintOptions, PrintOptions, PrintProxy, Settings};

/// What became of a print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Printed {
    /// Sent to the printer chosen in the print dialog.
    Sent,
    /// The print dialog was closed without printing.
    Cancelled,
    /// Opened in the PDF viewer (no print dialog here).
    Viewer,
}

/// Print the PDF at `pdf`; `title` names it in the print dialog.
pub(crate) async fn print(pdf: &Path, title: &str) -> Result<Printed, String> {
    match through_portal(pdf, title).await {
        Ok(printed) => Ok(printed),
        Err(err) => {
            log::warn!("print through the desktop portal failed: {err}");
            let opened = std::process::Command::new("xdg-open").arg(pdf).status().map_err(|e| format!("xdg-open: {e}"))?;
            if opened.success() { Ok(Printed::Viewer) } else { Err(format!("xdg-open: {opened}")) }
        }
    }
}

/// The portal's two steps: the print dialog (its settings come back with a
/// token), then the file to print with them.
async fn through_portal(pdf: &Path, title: &str) -> Result<Printed, String> {
    let file = std::fs::File::open(pdf).map_err(|e| format!("{}: {e}", pdf.display()))?;
    let proxy = PrintProxy::new().await.map_err(|e| e.to_string())?;
    let dialog = proxy
        .prepare_print(None, title, Settings::default(), PageSetup::default(), PreparePrintOptions::default().set_modal(true))
        .await
        .map_err(|e| e.to_string())?;
    let prepared = match dialog.response() {
        Ok(prepared) => prepared,
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(Printed::Cancelled),
        Err(err) => return Err(err.to_string()),
    };
    let sent = proxy.print(None, title, &file, PrintOptions::default().set_token(prepared.token)).await.map_err(|e| e.to_string())?;
    match sent.response() {
        Ok(()) => Ok(Printed::Sent),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(Printed::Cancelled),
        Err(err) => Err(err.to_string()),
    }
}
