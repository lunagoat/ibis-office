// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A window's title bar on Linux.
//!
//! Every window asks the desktop to draw its title bar (`decorations`:
//! server-side decorations), as macOS and Windows draw theirs. KDE, the
//! wlroots compositors, Hyprland and the X11 window managers do. GNOME's
//! Wayland session draws none: there a window is "client-side decorated"
//! and draws `title_bar` itself, above the rest: its title, dragged to move
//! the window, double-clicked to maximize it, with the buttons the desktop
//! allows (gpui-component's `TitleBar`). The edges such a window is resized
//! by and its shadow are gpui-component's too (`Root`).
//!
//! `IBIS_WINDOW_DECORATIONS=client` asks for client-side decorations
//! everywhere (for trying this bar on another desktop), `server` is the
//! default.

use gpui_kit::*;

/// What a window asks the desktop for (`WindowOptions::window_decorations`).
/// `None` where there is nothing to ask (macOS, Windows, the browser).
pub(crate) fn decorations() -> Option<WindowDecorations> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let client = std::env::var("IBIS_WINDOW_DECORATIONS").is_ok_and(|v| v.eq_ignore_ascii_case("client"));
    Some(if client { WindowDecorations::Client } else { WindowDecorations::Server })
}

/// The title bar of a window titled `title`, for the top of its view: `None`
/// where the desktop draws the window's (and in full screen, which has none).
#[cfg(target_os = "linux")]
pub(crate) fn title_bar(title: impl Into<SharedString>, window: &Window) -> Option<AnyElement> {
    use gpui_kit::component::TitleBar;
    if window.is_fullscreen() || !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return None;
    }
    let title: SharedString = title.into();
    Some(
        TitleBar::new()
            .bg(rgb(crate::shell::chrome::APP_BG))
            .text_color(rgb(crate::shell::chrome::TEXT))
            // The title in the middle of what the buttons leave.
            .child(div().flex_1().min_w_0().px(px(8.)).text_size(px(13.)).text_center().truncate().child(title))
            .into_any_element(),
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn title_bar(_title: impl Into<SharedString>, _window: &Window) -> Option<AnyElement> {
    None
}
