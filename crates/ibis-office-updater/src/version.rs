// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! version.txt: a platform, a tab and its version per line (`macOS<TAB>0.8.0`,
//! `Windows<TAB>0.8.0`, `wasm<TAB>0.8.0`, `Linux<TAB>0.8.0`), and comparing versions.

/// This platform's name in version.txt and in the zip's (Linux: the tarball's) file name.
pub const PLATFORM: &str = if cfg!(target_os = "windows") {
    "Windows"
} else if cfg!(target_os = "linux") {
    "Linux"
} else {
    "macOS"
};

/// `platform`'s version in the text of version.txt.
pub fn find(text: &str, platform: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.trim_end_matches('\r').split_once('\t'))
        .find(|(name, _)| name.trim() == platform)
        .map(|(_, version)| version.trim().to_string())
        .filter(|v| valid(v))
}

/// Digits and dots only (`0.9.0`): the version goes into a file name and a URL.
pub fn valid(version: &str) -> bool {
    !version.is_empty() && version.split('.').all(|part| !part.is_empty() && part.len() <= 9 && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `candidate` is newer than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    parts(candidate) > parts(current)
}

fn parts(version: &str) -> Vec<u64> {
    let mut parts: Vec<u64> = version.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect();
    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_platform() {
        let text = "Windows\t0.8.1\r\nmacOS\t0.9.0\r\nwasm\t0.8.0\r\n";
        assert_eq!(find(text, "macOS").as_deref(), Some("0.9.0"));
        assert_eq!(find(text, "Windows").as_deref(), Some("0.8.1"));
        assert_eq!(find(text, "Linux"), None);
        assert_eq!(find("macOS\t../../etc\n", "macOS"), None);
    }

    #[test]
    fn linux_has_a_line_of_its_own() {
        // Upstream's version.txt has no Linux line: a Linux build that asks it is offered nothing.
        assert_eq!(find("Windows\t0.9.0\nmacOS\t0.9.0\nwasm\t0.9.0\n", "Linux"), None);
        assert_eq!(find("Windows\t0.9.0\nmacOS\t0.9.0\nwasm\t0.9.0\nLinux\t0.8.2\n", "Linux").as_deref(), Some("0.8.2"));
        #[cfg(target_os = "linux")]
        assert_eq!(PLATFORM, "Linux");
    }

    #[test]
    fn compares_numbers_not_text() {
        assert!(is_newer("0.9.0", "0.8.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("0.8.0", "0.8"));
        assert!(!is_newer("0.8.0", "0.8.1"));
    }
}
