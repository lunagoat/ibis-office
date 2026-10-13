// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Replacing an installed folder with a new one, and putting it back, for
//! macOS (the `.app`), Windows (the folder holding `ibisOffice.exe` and
//! what goes with it) and Linux (the `ibisOffice` folder holding the
//! `ibisOffice` executable). Done here in Rust, not by a script, so that a change
//! of the steps is a new ibisOfficeUpdater (the one in the new version is
//! the one that runs). The order is what keeps the
//! installed copy whole at every moment:
//!
//! 1. `install`: copy the new folder next to the old one (`<old>.update-new`),
//!    move the old one aside (`<old>.update-backup`), move the copy into
//!    place. Anything failing puts the old one back.
//! 2. `rollback`: the old one back in place (the new one did not start).
//! 3. `cleanup`: the backup removed (the new one started).

use std::path::{Path, PathBuf};

fn beside(old: &Path, suffix: &str) -> PathBuf {
    let mut name = old.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    old.with_file_name(name)
}

/// The old copy, moved aside by `install`.
pub fn backup_of(old: &Path) -> PathBuf {
    beside(old, ".update-backup")
}

fn staged_of(old: &Path) -> PathBuf {
    beside(old, ".update-new")
}

/// Copy a folder. macOS: `ditto`, which keeps everything a signed .app has (symbolic links, extended attributes).
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/bin/ditto").arg(from).arg(to).output().map_err(|e| format!("ditto: {e}"))?;
        if out.status.success() { Ok(()) } else { Err(format!("ditto {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim())) }
    }
    #[cfg(not(target_os = "macos"))]
    copy_dir(from, to).map_err(|e| e.to_string())
}

#[allow(dead_code)] // macOS copies with ditto
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&src, &dst)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
            #[cfg(not(unix))]
            {
                std::fs::copy(&src, &dst)?;
            }
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Put `new` in place of `old`. `check` looks at the copy (made next to `old`) before anything is replaced. On an error `old` is as it was.
pub fn install(old: &Path, new: &Path, check: &dyn Fn(&Path) -> Result<(), String>) -> Result<(), String> {
    let (staged, backup) = (staged_of(old), backup_of(old));
    let _ = std::fs::remove_dir_all(&staged);
    let _ = std::fs::remove_dir_all(&backup);
    if let Err(err) = copy_tree(new, &staged) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("copying {} to {}: {err}", new.display(), staged.display()));
    }
    if let Err(err) = check(&staged) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("the copy {} is not good: {err}", staged.display()));
    }
    if let Err(err) = std::fs::rename(old, &backup) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("moving {} aside: {err}", old.display()));
    }
    if let Err(err) = std::fs::rename(&staged, old) {
        let back = std::fs::rename(&backup, old);
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("moving the new copy into place: {err}{}", back.err().map(|e| format!(" (and putting the old one back: {e})")).unwrap_or_default()));
    }
    Ok(())
}

/// The old copy back in place of the new one.
pub fn rollback(old: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if !backup.is_dir() {
        return Err(format!("{} is not there", backup.display()));
    }
    std::fs::remove_dir_all(old).map_err(|e| format!("removing the new copy {}: {e}", old.display()))?;
    std::fs::rename(&backup, old).map_err(|e| format!("moving {} back: {e}", backup.display()))
}

/// The backup removed.
pub fn cleanup(old: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if backup.exists() {
        std::fs::remove_dir_all(&backup).map_err(|e| format!("removing {}: {e}", backup.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ibis-updater-swap-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("old/sub")).unwrap();
        std::fs::create_dir_all(dir.join("new/sub")).unwrap();
        std::fs::write(dir.join("old/app.exe"), "old").unwrap();
        std::fs::write(dir.join("old/sub/data"), "old data").unwrap();
        std::fs::write(dir.join("new/app.exe"), "new").unwrap();
        std::fs::write(dir.join("new/sub/data"), "new data").unwrap();
        dir
    }

    #[test]
    fn install_then_cleanup() {
        let dir = tree("ok");
        install(&dir.join("old"), &dir.join("new"), &|_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join("old/sub/data")).unwrap(), "new data");
        assert_eq!(std::fs::read_to_string(backup_of(&dir.join("old")).join("app.exe")).unwrap(), "old");
        cleanup(&dir.join("old")).unwrap();
        assert!(!backup_of(&dir.join("old")).exists());
        assert!(dir.join("new").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_that_fails_the_check_leaves_the_old_one() {
        let dir = tree("check");
        assert!(install(&dir.join("old"), &dir.join("new"), &|_| Err("bad".into())).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!staged_of(&dir.join("old")).exists() && !backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_puts_the_old_one_back() {
        let dir = tree("rollback");
        install(&dir.join("old"), &dir.join("new"), &|_| Ok(())).unwrap();
        rollback(&dir.join("old")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Linux: what makes the folder an app is kept by the copy (a program that cannot be run, or a link turned into a file, would be a broken install), and a path that points into the folder still does after the swap.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_copy_keeps_the_executable_bit_and_links() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tree("linux");
        let executable = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0;
        std::fs::set_permissions(dir.join("new/app.exe"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("sub/data", dir.join("new/link")).unwrap();
        // The launcher: a link from outside the folder to the program in it.
        std::os::unix::fs::symlink(dir.join("old/app.exe"), dir.join("launcher")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("launcher")).unwrap(), "old");

        install(&dir.join("old"), &dir.join("new"), &|staged| if executable(&staged.join("app.exe")) { Ok(()) } else { Err("not executable".into()) }).unwrap();
        assert!(executable(&dir.join("old/app.exe")));
        assert!(!executable(&dir.join("old/sub/data")));
        assert_eq!(std::fs::read_link(dir.join("old/link")).unwrap(), Path::new("sub/data"));
        assert_eq!(std::fs::read_to_string(dir.join("old/link")).unwrap(), "new data");
        assert_eq!(std::fs::read_to_string(dir.join("launcher")).unwrap(), "new");

        rollback(&dir.join("old")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("launcher")).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_new_folder_leaves_the_old_one() {
        let dir = tree("missing");
        assert!(install(&dir.join("old"), &dir.join("nothing"), &|_| Ok(())).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
