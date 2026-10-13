// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where the update's files go.
//!
//! - macOS: unpacked in `~/Library/Caches/jp.ne.ibis.office/Update/<version>/`,
//!   log `~/Library/Logs/ibisOffice/updater.log`.
//! - Windows: `%LOCALAPPDATA%\ibisOffice\Update\<version>\`,
//!   log `%LOCALAPPDATA%\ibisOffice\Logs\updater.log`.
//! - Linux (the XDG base directories): `$XDG_CACHE_HOME/ibisOffice/Update/<version>/`
//!   (`~/.cache` without the variable), log `$XDG_STATE_HOME/ibisOffice/updater.log`
//!   (`~/.local/state`).
//!
//! `IBIS_UPDATE_DIR` puts both under one folder instead (`Update/`, `Logs/`),
//! for the tests, which must not touch the real ones.

use std::path::PathBuf;

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// An XDG base directory: `name` if it is set to an absolute path (the specification says to ignore any other value), else `fallback` in the home folder.
fn xdg(env: &dyn Fn(&str) -> Option<PathBuf>, name: &str, fallback: &str) -> Option<PathBuf> {
    env(name).filter(|dir| dir.is_absolute()).or_else(|| Some(env("HOME")?.join(fallback)))
}

/// The folder the versions are unpacked into (one folder per version).
pub fn update_root() -> Option<PathBuf> {
    update_root_in(&env)
}

fn update_root_in(env: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = env("IBIS_UPDATE_DIR") {
        return Some(dir.join("Update"));
    }
    if cfg!(target_os = "windows") {
        Some(env("LOCALAPPDATA")?.join("ibisOffice").join("Update"))
    } else if cfg!(target_os = "linux") {
        Some(xdg(env, "XDG_CACHE_HOME", ".cache")?.join("ibisOffice").join("Update"))
    } else {
        Some(env("HOME")?.join("Library/Caches/jp.ne.ibis.office/Update"))
    }
}

/// Where version `version` is unpacked.
pub fn update_dir(version: &str) -> Option<PathBuf> {
    Some(update_root()?.join(version))
}

/// `updater.log`.
pub fn log_file() -> Option<PathBuf> {
    log_file_in(&env)
}

fn log_file_in(env: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = env("IBIS_UPDATE_DIR") {
        return Some(dir.join("Logs").join("updater.log"));
    }
    if cfg!(target_os = "windows") {
        Some(env("LOCALAPPDATA")?.join("ibisOffice").join("Logs").join("updater.log"))
    } else if cfg!(target_os = "linux") {
        Some(xdg(env, "XDG_STATE_HOME", ".local/state")?.join("ibisOffice").join("updater.log"))
    } else {
        Some(env("HOME")?.join("Library/Logs/ibisOffice/updater.log"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An environment of the variables given, for a test (the real one is shared by every test running).
    fn vars(set: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<PathBuf> {
        move |name| set.iter().find(|(n, v)| *n == name && !v.is_empty()).map(|(_, v)| PathBuf::from(v))
    }

    #[test]
    fn ibis_update_dir_holds_both() {
        let env = vars(&[("IBIS_UPDATE_DIR", "/t"), ("HOME", "/home/u"), ("LOCALAPPDATA", "/l"), ("XDG_CACHE_HOME", "/c"), ("XDG_STATE_HOME", "/s")]);
        assert_eq!(update_root_in(&env), Some(PathBuf::from("/t/Update")));
        assert_eq!(log_file_in(&env), Some(PathBuf::from("/t/Logs/updater.log")));
    }

    #[cfg(unix)] // `/c` is not an absolute path on Windows
    #[test]
    fn an_xdg_directory_is_the_variable_or_its_default_at_home() {
        assert_eq!(xdg(&vars(&[("XDG_CACHE_HOME", "/c"), ("HOME", "/home/u")]), "XDG_CACHE_HOME", ".cache"), Some(PathBuf::from("/c")));
        assert_eq!(xdg(&vars(&[("HOME", "/home/u")]), "XDG_CACHE_HOME", ".cache"), Some(PathBuf::from("/home/u/.cache")));
        // Not absolute: ignored, as the specification says.
        assert_eq!(xdg(&vars(&[("XDG_CACHE_HOME", "cache"), ("HOME", "/home/u")]), "XDG_CACHE_HOME", ".cache"), Some(PathBuf::from("/home/u/.cache")));
        assert_eq!(xdg(&vars(&[("XDG_CACHE_HOME", ""), ("HOME", "/home/u")]), "XDG_CACHE_HOME", ".cache"), Some(PathBuf::from("/home/u/.cache")));
        assert_eq!(xdg(&vars(&[]), "XDG_CACHE_HOME", ".cache"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_follows_xdg() {
        let home = vars(&[("HOME", "/home/u")]);
        assert_eq!(update_root_in(&home), Some(PathBuf::from("/home/u/.cache/ibisOffice/Update")));
        assert_eq!(log_file_in(&home), Some(PathBuf::from("/home/u/.local/state/ibisOffice/updater.log")));
        let set = vars(&[("HOME", "/home/u"), ("XDG_CACHE_HOME", "/c"), ("XDG_STATE_HOME", "/s")]);
        assert_eq!(update_root_in(&set), Some(PathBuf::from("/c/ibisOffice/Update")));
        assert_eq!(log_file_in(&set), Some(PathBuf::from("/s/ibisOffice/updater.log")));
        // Nowhere to put them: no update, rather than a path made up.
        assert_eq!(update_root_in(&vars(&[])), None);
        assert_eq!(log_file_in(&vars(&[])), None);
    }
}
