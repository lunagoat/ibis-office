// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スペルチェック: the words of a text the system's spell checker does not
//! know (`NSSpellChecker` on macOS, with the user's own dictionary), and what
//! it suggests for them. Linux has no spell checker of its own: the Hunspell
//! dictionaries installed are read (`linux` below), with a word list of the
//! user's own. Other platforms check nothing yet.

/// A word the checker does not know: where it is in the text, in characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Misspelling {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) word: String,
}

/// Whether the system can check spelling (on Linux: a dictionary is installed).
pub(crate) fn available() -> bool {
    #[cfg(target_os = "linux")]
    return linux::available();
    #[cfg(not(target_os = "linux"))]
    cfg!(target_os = "macos")
}

/// The words of `text` not known in `language` (`None`: the system's own choice from the text).
pub(crate) fn check(text: &str, language: Option<&str>) -> Vec<Misspelling> {
    #[cfg(target_os = "macos")]
    return mac::check(text, language);
    #[cfg(target_os = "linux")]
    return linux::check(text, language);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (text, language);
        Vec::new()
    }
}

/// What the checker suggests for the word at characters `start..end` of `text`.
pub(crate) fn suggestions(text: &str, start: usize, end: usize, language: Option<&str>) -> Vec<String> {
    #[cfg(target_os = "macos")]
    return mac::suggestions(text, start, end, language);
    #[cfg(target_os = "linux")]
    return linux::suggestions(text, start, end, language);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (text, start, end, language);
        Vec::new()
    }
}

/// 辞書に追加: `word` learned by the system's dictionary of the user. In tests the
/// word is only noted (`learned_in_tests`): the user's own dictionary is not touched.
pub(crate) fn learn(word: &str) {
    #[cfg(any(test, feature = "scenarios"))]
    {
        LEARNED.with(|l| l.borrow_mut().push(word.to_string()));
        return;
    }
    #[cfg(all(target_os = "macos", not(any(test, feature = "scenarios"))))]
    mac::learn(word);
    #[cfg(all(target_os = "linux", not(any(test, feature = "scenarios"))))]
    linux::learn(word);
    #[cfg(all(not(any(target_os = "macos", target_os = "linux")), not(any(test, feature = "scenarios"))))]
    let _ = word;
}

#[cfg(any(test, feature = "scenarios"))]
thread_local! {
    static LEARNED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The words `learn` was asked for in this test.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) fn learned_in_tests() -> Vec<String> {
    LEARNED.with(|l| l.borrow().clone())
}

/// The offset in UTF-16 units that characters `chars` of `text` come to.
pub(crate) fn utf16_offset(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

/// The character count that UTF-16 offset `units` of `text` is.
pub(crate) fn char_offset(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.chars().enumerate() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.chars().count()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    use objc::runtime::{BOOL, NO, Object};
    use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};

    use super::{Misspelling, char_offset, utf16_offset};

    type Id = *mut Object;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRange {
        location: usize,
        length: usize,
    }

    unsafe impl Encode for NSRange {
        fn encode() -> Encoding {
            unsafe { Encoding::from_str("{_NSRange=QQ}") }
        }
    }

    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    const NOT_FOUND: usize = isize::MAX as usize;

    fn string(text: &str) -> Id {
        unsafe {
            let s: Id = msg_send![class!(NSString), alloc];
            let s: Id = msg_send![s, initWithBytes: text.as_ptr() as *const c_void length: text.len() encoding: 4usize];
            msg_send![s, autorelease]
        }
    }

    fn rust_string(s: Id) -> String {
        unsafe {
            let bytes: *const std::os::raw::c_char = msg_send![s, UTF8String];
            if bytes.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr(bytes).to_string_lossy().into_owned()
        }
    }

    fn checker() -> Id {
        unsafe { msg_send![class!(NSSpellChecker), sharedSpellChecker] }
    }

    fn language_arg(language: Option<&str>) -> Id {
        language.map_or(std::ptr::null_mut(), string)
    }

    pub(super) fn check(text: &str, language: Option<&str>) -> Vec<Misspelling> {
        let mut found = Vec::new();
        if text.trim().is_empty() {
            return found;
        }
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let checker = checker();
            let ns = string(text);
            let language = language_arg(language);
            let total = utf16_offset(text, usize::MAX);
            let mut at = 0usize;
            while at < total {
                let range: NSRange = msg_send![checker, checkSpellingOfString: ns startingAt: at as isize language: language wrap: NO inSpellDocumentWithTag: 0isize wordCount: std::ptr::null_mut::<isize>()];
                if range.location >= NOT_FOUND || range.length == 0 {
                    break;
                }
                let (start, end) = (char_offset(text, range.location), char_offset(text, range.location + range.length));
                let word: String = text.chars().skip(start).take(end - start).collect();
                found.push(Misspelling { start, end, word });
                at = range.location + range.length;
            }
            let _: () = msg_send![pool, drain];
        }
        found
    }

    pub(super) fn suggestions(text: &str, start: usize, end: usize, language: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let checker = checker();
            let (a, b) = (utf16_offset(text, start), utf16_offset(text, end));
            let range = NSRange { location: a, length: b - a };
            let guesses: Id = msg_send![checker, guessesForWordRange: range inString: string(text) language: language_arg(language) inSpellDocumentWithTag: 0isize];
            if !guesses.is_null() {
                let count: usize = msg_send![guesses, count];
                for i in 0..count.min(8) {
                    let item: Id = msg_send![guesses, objectAtIndex: i];
                    out.push(rust_string(item));
                }
            }
            let _: () = msg_send![pool, drain];
        }
        out
    }

    #[cfg_attr(any(test, feature = "scenarios"), allow(dead_code))]
    pub(super) fn learn(word: &str) {
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let _: () = msg_send![checker(), learnWord: string(word)];
            let _: () = msg_send![pool, drain];
        }
    }

    #[allow(dead_code)]
    const _: BOOL = NO;
}

/// Linux: the Hunspell dictionaries installed (`<language>.aff` with its
/// `.dic`), read by spellbook, which checks and suggests from them in Rust:
/// no library is linked, and where no dictionary is installed nothing is
/// misspelled. A dictionary is read when a text is first checked in it, once
/// for the process.
#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

    use super::Misspelling;
    use crate::shell::i18n::{self, Language};

    /// The user's own words (辞書に追加), one per line, beside the settings
    /// (`shell/settings.rs`: `~/.config/ibisOffice`, or where `IBIS_SETTINGS_DIR` says).
    const WORDS_FILE: &str = "user-dictionary.txt";

    const APOSTROPHES: [char; 2] = ['\'', '’'];

    /// A dictionary read: its words, and the characters they are written in.
    struct Dictionary {
        words: spellbook::Dictionary,
        letters: HashSet<char>,
    }

    /// A dictionary installed.
    struct Installed {
        /// The files' name (`en_US`), and the same as languages are compared (`en_us`).
        name: String,
        key: String,
        aff: PathBuf,
        dic: PathBuf,
        read: Read,
    }

    /// Reading takes time and memory: done when first asked for, and not again if it failed.
    enum Read {
        NotYet,
        Failed,
        Done(Dictionary),
    }

    struct Speller {
        folders: Vec<PathBuf>,
        installed: Vec<Installed>,
        words_file: Option<PathBuf>,
        learned: Vec<String>,
        /// The languages already logged as having no dictionary.
        told: HashSet<String>,
    }

    fn env(name: &str) -> Option<OsString> {
        std::env::var_os(name).filter(|v| !v.is_empty())
    }

    /// The checker of the process, made at its first use.
    fn shared() -> MutexGuard<'static, Speller> {
        static SHARED: OnceLock<Mutex<Speller>> = OnceLock::new();
        SHARED.get_or_init(|| Mutex::new(Speller::new(folders(env), words_file(env)))).lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn available() -> bool {
        let languages = languages(None);
        shared().dictionary(&languages).is_some()
    }

    pub(super) fn check(text: &str, language: Option<&str>) -> Vec<Misspelling> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let languages = languages(language);
        shared().check(text, &languages)
    }

    pub(super) fn suggestions(text: &str, start: usize, end: usize, language: Option<&str>) -> Vec<String> {
        let languages = languages(language);
        shared().suggestions(text, start, end, &languages)
    }

    #[cfg_attr(any(test, feature = "scenarios"), allow(dead_code))]
    pub(super) fn learn(word: &str) {
        shared().learn(word);
    }

    /// Where Hunspell dictionaries are looked for, first to last: the folders
    /// `$DICPATH` names (as Hunspell itself does), the user's own, then those
    /// of the system (distributions install into `hunspell` or `myspell`).
    fn folders(env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
        let named = |paths: OsString| std::env::split_paths(&paths).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>();
        let mut folders = env("DICPATH").map(named).unwrap_or_default();
        if let Some(data) = env("XDG_DATA_HOME").map(PathBuf::from).or_else(|| env("HOME").map(|home| PathBuf::from(home).join(".local/share"))) {
            folders.push(data.join("hunspell"));
        }
        for data in named(env("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into())) {
            folders.extend(["hunspell", "myspell", "myspell/dicts"].map(|name| data.join(name)));
        }
        folders
    }

    /// Where the user's own words are kept: in the folder of the settings.
    fn words_file(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
        let env = |name: &str| env(name).map(PathBuf::from);
        if let Some(dir) = env("IBIS_SETTINGS_DIR") {
            return Some(dir.join(WORDS_FILE));
        }
        let base = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|home| home.join(".config")));
        Some(base?.join("ibisOffice").join(WORDS_FILE))
    }

    /// The dictionaries in `folders`: every `.aff` that has its `.dic`. Of
    /// two of the same name, the one in the earlier folder.
    fn installed(folders: &[PathBuf]) -> Vec<Installed> {
        let mut found: Vec<Installed> = Vec::new();
        for folder in folders {
            let Ok(entries) = std::fs::read_dir(folder) else { continue };
            let mut names: Vec<String> = entries.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".aff").map(str::to_string)).collect();
            names.sort();
            for name in names {
                let (aff, dic) = (folder.join(format!("{name}.aff")), folder.join(format!("{name}.dic")));
                if aff.is_file() && dic.is_file() && !found.iter().any(|d| d.name == name) {
                    found.push(Installed { key: name.replace(['-', '@'], "_").to_lowercase(), name, aff, dic, read: Read::NotYet });
                }
            }
        }
        found
    }

    /// A language as dictionaries are named, in lower case: `en-US` and
    /// `en_US.UTF-8` are `en_us`. None for a locale that names no language.
    fn key(language: &str) -> Option<String> {
        let name = language.split(['.', '@']).next().unwrap_or_default().trim();
        (!name.is_empty() && !name.eq_ignore_ascii_case("C") && !name.eq_ignore_ascii_case("POSIX")).then(|| name.replace('-', "_").to_lowercase())
    }

    /// The languages to check in, the most wanted first (the first that has a
    /// dictionary is taken): the caller's and no other. With none, where
    /// macOS leaves the choice to the system: the system's (`$LC_ALL`,
    /// `$LC_MESSAGES`, `$LANG`), the UI's, then American English.
    fn languages(language: Option<&str>) -> Vec<String> {
        if let Some(language) = language {
            return vec![language.to_string()];
        }
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
        system_choice(i18n::language(), i18n::system_language(), locale)
    }

    /// The languages for a text whose language the caller does not name. The
    /// UI's language comes from the system's unless the user chose another
    /// in the settings: only then it is the first, as the system's says more
    /// (the region, or a language the UI has no text in). American English
    /// is the last, the dictionary most systems have: where the language has
    /// none (Japanese), the English words of a text are still checked.
    fn system_choice(ui: Language, system: Language, locale: Option<String>) -> Vec<String> {
        let mut languages: Vec<String> = locale.into_iter().collect();
        let at = if ui == system { languages.len() } else { 0 };
        languages.insert(at, if ui == Language::Japanese { "ja" } else { "en" }.to_string());
        languages.push("en_US".to_string());
        languages
    }

    /// The encoding a `.aff` names for itself and its `.dic` (`SET UTF-8`,
    /// `SET ISO8859-15`, `SET microsoft-cp1251`); one that names none is in
    /// ISO 8859-1, as Hunspell reads it.
    fn encoding(aff: &[u8]) -> Result<&'static encoding_rs::Encoding, String> {
        let lines = aff.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(aff).split(|b| *b == b'\n');
        let Some(name) = lines.filter_map(|line| line.trim_ascii().strip_prefix(b"SET")).find(|name| name.first().is_some_and(u8::is_ascii_whitespace)) else {
            return Ok(encoding_rs::WINDOWS_1252);
        };
        let name = name.trim_ascii();
        encoding_rs::Encoding::for_label(name.strip_prefix(b"microsoft-").unwrap_or(name)).ok_or_else(|| format!("the encoding {} is not known", String::from_utf8_lossy(name)))
    }

    /// The characters the words of a dictionary are written in: the letters
    /// of its stems, in both cases, and what else is in them beyond ASCII
    /// that is no letter (the dot of Catalan's l·l), but for dashes and
    /// quotation marks. The hyphens, dots and digits of ASCII are left out
    /// too: words end at them.
    fn letters(aff: &str, dic: &str) -> HashSet<char> {
        let mut letters = HashSet::new();
        let mut add = |text: &str| {
            for c in text.chars().filter(|c| c.is_alphabetic() || !(c.is_ascii() || c.is_whitespace() || ('\u{2010}'..='\u{205E}').contains(c))) {
                if letters.insert(c) {
                    letters.extend(c.to_lowercase().chain(c.to_uppercase()));
                }
            }
        };
        // The first line is the number of words; after the stem come its flags.
        for line in dic.lines().skip(1) {
            add(line.split(['/', '\t']).next().unwrap_or_default());
        }
        // What the `.aff` says of the language's characters, and of those typed for them.
        for line in aff.lines() {
            if let Some(characters) = ["TRY ", "WORDCHARS ", "ICONV "].iter().find_map(|option| line.strip_prefix(option)) {
                add(characters);
            }
        }
        letters
    }

    /// Kanji, kana, hangul and the full-width forms: a word of another
    /// script ends where they begin, as Japanese is written without spaces
    /// around the English words in it.
    fn east_asian(c: char) -> bool {
        matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
    }

    /// The words of `text` for a dictionary written in `letters` (the first
    /// character, one past the last, the word): runs of letters and digits
    /// with the apostrophes inside them. A soft hyphen, a combining accent
    /// or a joiner stays in its word, which is then not one the dictionary
    /// judges (`Dictionary::judges`) rather than two pieces it does not know.
    fn words_of<'a>(text: &'a str, letters: &HashSet<char>) -> Vec<(usize, usize, &'a str)> {
        let in_word = |c: char| {
            (c.is_alphanumeric() && !east_asian(c)) || APOSTROPHES.contains(&c) || matches!(c, '\u{AD}' | '\u{300}'..='\u{36F}' | '\u{200C}' | '\u{200D}') || (!c.is_ascii() && letters.contains(&c))
        };
        let mut found = Vec::new();
        let mut word = |first: usize, run: &'a str| {
            let rest = run.trim_start_matches(APOSTROPHES);
            let first = first + run[..run.len() - rest.len()].chars().count();
            let word = rest.trim_end_matches(APOSTROPHES);
            if !word.is_empty() {
                found.push((first, first + word.chars().count(), word));
            }
        };
        // Characters before the piece.
        let mut before = 0;
        for piece in text.split_inclusive(char::is_whitespace) {
            // A mail or web address is not prose.
            let address = piece.contains('@') || piece.contains("://") || piece.contains("www.");
            let (mut count, mut start) = (0, None);
            for (byte, c) in piece.char_indices() {
                if !address && in_word(c) {
                    start.get_or_insert((before + count, byte));
                } else if let Some((first, from)) = start.take() {
                    word(first, &piece[from..byte]);
                }
                count += 1;
            }
            if let Some((first, from)) = start {
                word(first, &piece[from..]);
            }
            before += count;
        }
        found
    }

    impl Dictionary {
        fn read(aff: &Path, dic: &Path) -> Result<Self, String> {
            let (aff, dic) = (std::fs::read(aff).map_err(|err| err.to_string())?, std::fs::read(dic).map_err(|err| format!("{}: {err}", dic.display()))?);
            // spellbook reads text: a dictionary in another encoding is decoded first.
            let encoding = encoding(&aff)?;
            let (aff, dic) = (encoding.decode(&aff).0, encoding.decode(&dic).0);
            let words = spellbook::Dictionary::new(&aff, &dic).map_err(|err| err.to_string())?;
            Ok(Self { words, letters: letters(&aff, &dic) })
        }

        /// Whether `word` is one this dictionary can tell about: two letters
        /// or more, all of them characters its own words are written in. A
        /// word of another script (Japanese or Russian for an English
        /// dictionary), one with a digit, a single letter: not misspelled.
        fn judges(&self, word: &str) -> bool {
            let mut letters = word.chars().filter(|c| !APOSTROPHES.contains(c));
            letters.clone().count() >= 2 && letters.all(|c| self.letters.contains(&c))
        }

        fn knows(&self, word: &str) -> bool {
            // The typographic apostrophe, for a dictionary that does not convert it itself.
            self.words.check(word) || (word.contains('’') && self.words.check(&word.replace('’', "'")))
        }

        /// The user's own word. One with a `/` would be read as a stem and its flags: it is not taken.
        fn learn(&mut self, word: &str) {
            if !word.contains(['/', '\\']) {
                let _ = self.words.add(word);
            }
        }
    }

    impl Speller {
        fn new(folders: Vec<PathBuf>, words_file: Option<PathBuf>) -> Self {
            let installed = installed(&folders);
            let learned = words_file.as_deref().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default().lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_string).collect();
            Self { folders, installed, words_file, learned, told: HashSet::new() }
        }

        /// The dictionary installed for `language`: of its region (`en_GB`)
        /// or a variant of it (`en_GB-ise`), else of the language: `en`
        /// itself, the region most dictionaries of a language are named
        /// for (`en_US`, `de_DE`), or the first there is.
        fn find(&self, language: &str) -> Option<usize> {
            let wanted = key(language)?;
            let language = wanted.split('_').next().unwrap_or_default();
            let usual = if language == "en" { "en_us".to_string() } else { format!("{language}_{language}") };
            let variant_of = |key: &str, name: &str| key.strip_prefix(name).is_some_and(|rest| rest.starts_with('_'));
            let rank = |key: &str| match key {
                _ if key == wanted => Some(0),
                _ if wanted != language && variant_of(key, &wanted) => Some(1),
                _ if key == language => Some(2),
                _ if key == usual => Some(3),
                _ if variant_of(key, language) => Some(4),
                _ => None,
            };
            self.installed.iter().enumerate().filter_map(|(at, d)| Some((rank(&d.key)?, at))).min().map(|(_, at)| at)
        }

        /// The dictionary to check in: the one installed for the first of
        /// `languages` that has one. None is logged once with the folders
        /// looked in: what to install, or where, is not told otherwise.
        fn dictionary(&mut self, languages: &[String]) -> Option<&Dictionary> {
            let Some(at) = languages.iter().find_map(|language| self.find(language).filter(|at| self.read(*at))) else {
                if self.told.insert(languages.join(", ")) {
                    let folders: Vec<String> = self.folders.iter().map(|f| f.display().to_string()).collect();
                    log::info!("spelling: no Hunspell dictionary (.aff and .dic) for {} in {}", languages.join(", "), folders.join(", "));
                }
                return None;
            };
            match &self.installed[at].read {
                Read::Done(dictionary) => Some(dictionary),
                _ => None,
            }
        }

        /// Whether the dictionary numbered `at` is read: now, if it has not been tried.
        fn read(&mut self, at: usize) -> bool {
            let installed = &mut self.installed[at];
            if matches!(installed.read, Read::NotYet) {
                installed.read = match Dictionary::read(&installed.aff, &installed.dic) {
                    Ok(mut dictionary) => {
                        self.learned.iter().for_each(|word| dictionary.learn(word));
                        log::info!("spelling: {} read from {}", installed.name, installed.dic.display());
                        Read::Done(dictionary)
                    }
                    Err(err) => {
                        log::warn!("{}: {err}", installed.aff.display());
                        Read::Failed
                    }
                };
            }
            matches!(installed.read, Read::Done(_))
        }

        fn check(&mut self, text: &str, languages: &[String]) -> Vec<Misspelling> {
            let Some(dictionary) = self.dictionary(languages) else { return Vec::new() };
            let unknown = words_of(text, &dictionary.letters).into_iter().filter(|(_, _, word)| dictionary.judges(word) && !dictionary.knows(word));
            unknown.map(|(start, end, word)| Misspelling { start, end, word: word.to_string() }).collect()
        }

        fn suggestions(&mut self, text: &str, start: usize, end: usize, languages: &[String]) -> Vec<String> {
            let Some(dictionary) = self.dictionary(languages) else { return Vec::new() };
            let word: String = text.chars().skip(start).take(end.saturating_sub(start)).collect();
            let mut suggestions = Vec::new();
            dictionary.words.suggest(&word, &mut suggestions);
            // As many as macOS is asked for.
            suggestions.truncate(8);
            suggestions
        }

        /// `word` known from now on in every language, and kept in the
        /// user's word list: added to what the file has now, which the app
        /// started a second time may have added to.
        fn learn(&mut self, word: &str) {
            let word = word.trim();
            if word.is_empty() || self.learned.iter().any(|w| w == word) {
                return;
            }
            self.learned.push(word.to_string());
            for installed in &mut self.installed {
                if let Read::Done(dictionary) = &mut installed.read {
                    dictionary.learn(word);
                }
            }
            let Some(path) = &self.words_file else { return };
            let mut text = std::fs::read_to_string(path).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(word);
            text.push('\n');
            let result = match path.parent() {
                Some(dir) => std::fs::create_dir_all(dir).and_then(|()| std::fs::write(path, text)),
                None => std::fs::write(path, text),
            };
            if let Err(err) = result {
                log::warn!("{}: {err}", path.display());
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A small English dictionary: the tests that need no more than a
        /// few words run where none is installed.
        const AFF: &[u8] = b"SET UTF-8\nTRY abcdefghijklmnopqrstuvwxyz\n";
        const DIC: &[u8] = b"8\nhello\nworld\nthis\nis\nthe\nplan\nreceive\ntest\n";

        /// A folder of the test's own, with the dictionaries (name, `.aff`, `.dic`) in it.
        fn folder(test: &str, dictionaries: &[(&str, &[u8], &[u8])]) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("ibis-spelling-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            for (name, aff, dic) in dictionaries {
                std::fs::write(dir.join(format!("{name}.aff")), aff).unwrap();
                std::fs::write(dir.join(format!("{name}.dic")), dic).unwrap();
            }
            dir
        }

        fn english() -> [String; 1] {
            ["en_US".to_string()]
        }

        #[test]
        fn dictionaries_are_looked_for_in_dicpath_then_the_users_folder_then_the_systems() {
            let env = |name: &str| match name {
                "DICPATH" => Some(OsString::from("/a::/b")),
                "XDG_DATA_HOME" => Some("/home/u/data".into()),
                "XDG_DATA_DIRS" => Some("/x:/y".into()),
                _ => None,
            };
            let expected = ["/a", "/b", "/home/u/data/hunspell", "/x/hunspell", "/x/myspell", "/x/myspell/dicts", "/y/hunspell", "/y/myspell", "/y/myspell/dicts"];
            assert_eq!(folders(env), expected.map(PathBuf::from));
            // Nothing set but the home folder: the places the XDG variables stand for.
            let expected = ["/home/u/.local/share/hunspell", "/usr/local/share/hunspell", "/usr/local/share/myspell", "/usr/local/share/myspell/dicts", "/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts"];
            assert_eq!(folders(|name| (name == "HOME").then(|| "/home/u".into())), expected.map(PathBuf::from));
        }

        #[test]
        fn the_users_words_are_kept_beside_the_settings() {
            let home = |name: &str| (name == "HOME").then(|| OsString::from("/home/u"));
            assert_eq!(words_file(home), Some(PathBuf::from("/home/u/.config/ibisOffice/user-dictionary.txt")));
            let xdg = |name: &str| matches!(name, "HOME" | "XDG_CONFIG_HOME").then(|| OsString::from(if name == "HOME" { "/home/u" } else { "/cfg" }));
            assert_eq!(words_file(xdg), Some(PathBuf::from("/cfg/ibisOffice/user-dictionary.txt")));
            // The tests' folder for the settings takes the word list with it.
            let moved = |name: &str| Some(OsString::from(if name == "IBIS_SETTINGS_DIR" { "/elsewhere" } else { "/home/u" }));
            assert_eq!(words_file(moved), Some(PathBuf::from("/elsewhere/user-dictionary.txt")));
            assert_eq!(words_file(|_| None), None);
        }

        #[test]
        fn a_dictionary_is_found_by_its_language_in_the_first_folder_that_has_it() {
            let first = folder("find-first", &[("de_DE", AFF, DIC)]);
            let second = folder("find-second", &[("de_DE", AFF, DIC), ("en_GB", AFF, DIC), ("en_US", AFF, DIC), ("en_ZA", AFF, DIC), ("pt_BR", AFF, DIC)]);
            // Without its .dic it is no dictionary.
            std::fs::write(second.join("fr_FR.aff"), AFF).unwrap();
            let speller = Speller::new(vec![first.clone(), first.join("missing"), second.clone()], None);
            let found = |language: &str| speller.find(language).map(|at| speller.installed[at].aff.clone());
            for language in ["en_US", "en-US", "en_US.UTF-8", "en", "en_AU"] {
                assert_eq!(found(language), Some(second.join("en_US.aff")), "{language}");
            }
            assert_eq!(found("en-GB"), Some(second.join("en_GB.aff")));
            assert_eq!(found("pt"), Some(second.join("pt_BR.aff")));
            for language in ["de", "de_DE", "de_AT.UTF-8@euro"] {
                assert_eq!(found(language), Some(first.join("de_DE.aff")), "{language}");
            }
            for language in ["fr", "ja_JP.UTF-8", "e", "C", "POSIX", ""] {
                assert_eq!(found(language), None, "{language}");
            }
            let _ = (std::fs::remove_dir_all(first), std::fs::remove_dir_all(second));
        }

        #[test]
        fn the_language_not_named_is_the_systems_then_english() {
            use Language::{English, Japanese};
            assert_eq!(system_choice(Japanese, Japanese, Some("ja_JP.UTF-8".into())), ["ja_JP.UTF-8", "ja", "en_US"]);
            assert_eq!(system_choice(English, English, Some("de_DE.UTF-8".into())), ["de_DE.UTF-8", "en", "en_US"]);
            // English chosen in the settings of a Japanese system.
            assert_eq!(system_choice(English, Japanese, Some("ja_JP.UTF-8".into())), ["en", "ja_JP.UTF-8", "en_US"]);
            assert_eq!(system_choice(English, English, None), ["en", "en_US"]);
            assert_eq!(languages(Some("fr-FR")), ["fr-FR"]);
        }

        #[test]
        fn a_word_not_known_is_found_where_it_is_with_suggestions() {
            let dir = folder("check", &[("en_US", AFF, DIC)]);
            let mut speller = Speller::new(vec![dir.clone()], None);
            assert_eq!(speller.check("Hello world, this is the plan.", &english()), []);
            // In characters: the emoji is one (two UTF-16 units, four bytes).
            let text = "a😀 the recieve plan";
            assert_eq!(speller.check(text, &english()), [Misspelling { start: 7, end: 14, word: "recieve".into() }]);
            let suggestions = speller.suggestions(text, 7, 14, &english());
            assert!(suggestions.iter().any(|s| s == "receive"), "{suggestions:?}");
            // Quotation marks are not of the word; an address and a word with a digit are not checked.
            let found: Vec<(usize, String)> = speller.check("'wrold' x2z mail@wrold.test http://wrold.test/ (www.wrold.test)", &english()).into_iter().map(|m| (m.start, m.word)).collect();
            assert_eq!(found, [(1, "wrold".to_string())]);
            let _ = std::fs::remove_dir_all(dir);
        }

        #[test]
        fn a_word_is_whole_with_what_its_language_writes_inside_it() {
            // Catalan's dot is no letter; the dictionary has it in a word.
            let dir = folder("inside", &[("ca_ES", AFF, "2\ncol·legi\nplan\n".as_bytes()), ("en_US", AFF, DIC)]);
            let mut speller = Speller::new(vec![dir.clone()], None);
            let catalan = ["ca".to_string()];
            assert_eq!(speller.check("plan col·legi", &catalan), []);
            assert_eq!(speller.check("plan col·legu", &catalan), [Misspelling { start: 5, end: 13, word: "col·legu".into() }]);
            // A soft hyphen or a combining accent does not make two words of one
            // (which an English dictionary would not know: "wo", "rld").
            assert_eq!(speller.check("wo\u{AD}rld Zu\u{308}rich", &english()), []);
            assert_eq!(speller.check("wo rld", &english()).len(), 2);
            let _ = std::fs::remove_dir_all(dir);
        }

        #[test]
        fn text_in_a_language_without_a_dictionary_has_no_misspellings() {
            let dir = folder("languages", &[("en_US", AFF, DIC)]);
            let mut speller = Speller::new(vec![dir.clone()], None);
            // No German dictionary: nothing is told of a German text, not every word of it.
            let german = ["de_DE".to_string()];
            assert_eq!(speller.check("Das ist ein Tset", &german), []);
            assert_eq!(speller.suggestions("Das ist ein Tset", 12, 16, &german), Vec::<String>::new());
            // No dictionary at all.
            let mut none = Speller::new(vec![dir.join("none")], None);
            assert_eq!(none.check("Tihs is a tset", &system_choice(Language::English, Language::English, Some("en_US.UTF-8".into()))), []);
            assert!(none.dictionary(&english()).is_none());
            // A Japanese system checks in English: Japanese (and Russian) words are not
            // the English dictionary's to judge, the English words among them are.
            let japanese = system_choice(Language::Japanese, Language::Japanese, Some("ja_JP.UTF-8".into()));
            assert_eq!(speller.check("これは日本語の文章です。", &japanese), []);
            assert_eq!(speller.check("Привет, мир! Zürich", &japanese), []);
            assert_eq!(speller.check("新しいrecieveの計画", &japanese), [Misspelling { start: 3, end: 10, word: "recieve".into() }]);
            let _ = std::fs::remove_dir_all(dir);
        }

        #[test]
        fn a_learned_word_is_known_and_kept() {
            let dir = folder("learn", &[("en_US", AFF, DIC)]);
            // Not the user's own list: one in the test's folder (which `learn` makes).
            let list = dir.join("settings").join(WORDS_FILE);
            let mut speller = Speller::new(vec![dir.clone()], Some(list.clone()));
            assert_eq!(speller.check("the zzqxv plan", &english()), [Misspelling { start: 4, end: 9, word: "zzqxv".into() }]);
            speller.learn("zzqxv");
            assert_eq!(speller.check("the zzqxv plan. Zzqxv", &english()), []);
            assert_eq!(std::fs::read_to_string(&list).unwrap(), "zzqxv\n");
            // Read again (the app started again), with a word added by hand that has no line end.
            std::fs::write(&list, "zzqxv\nqqvzk").unwrap();
            let mut again = Speller::new(vec![dir.clone()], Some(list.clone()));
            assert_eq!(again.check("the zzqxv qqvzk wwxqj", &english()), [Misspelling { start: 16, end: 21, word: "wwxqj".into() }]);
            again.learn("wwxqj");
            again.learn("wwxqj");
            assert_eq!(again.check("the zzqxv qqvzk wwxqj", &english()), []);
            assert_eq!(std::fs::read_to_string(&list).unwrap(), "zzqxv\nqqvzk\nwwxqj\n");
            let _ = std::fs::remove_dir_all(dir);
        }

        #[test]
        fn a_dictionary_not_in_utf8_is_read() {
            // "café" in ISO 8859-1, which the .aff names.
            let dir = folder("latin1", &[("fr_FR", b"SET ISO8859-1\nTRY acf\xE9\n", b"2\ncaf\xE9\nface\n")]);
            let mut speller = Speller::new(vec![dir.clone()], None);
            let french = ["fr".to_string()];
            assert_eq!(speller.check("Café face", &french), []);
            assert_eq!(speller.check("caéf", &french), [Misspelling { start: 0, end: 4, word: "caéf".into() }]);
            // An encoding not known: the dictionary is not used.
            let dir2 = folder("encoding", &[("fr_FR", b"SET ISCII-DEVANAGARI\n", b"1\nface\n")]);
            assert_eq!(Speller::new(vec![dir2.clone()], None).check("caéf", &french), []);
            let _ = (std::fs::remove_dir_all(dir), std::fs::remove_dir_all(dir2));
        }

        /// The dictionaries installed (those of `DICPATH` first), if American English is among them.
        fn american() -> Option<Speller> {
            let speller = Speller::new(folders(env), None);
            if speller.find("en_US").is_some_and(|at| speller.installed[at].name == "en_US") {
                return Some(speller);
            }
            eprintln!("skipped: no en_US Hunspell dictionary is installed (DICPATH can name a folder with en_US.aff and en_US.dic)");
            None
        }

        #[test]
        fn american_english_is_checked_with_the_dictionary_installed() {
            let Some(mut speller) = american() else { return };
            let english = ["en-US".to_string()];
            assert_eq!(speller.check("The quick brown fox doesn't jump over the lazy dog's back; it's well-known, isn’t it? NASA said so in 2024.", &english), []);
            let text = "This is a recieve test";
            assert_eq!(speller.check(text, &english), [Misspelling { start: 10, end: 17, word: "recieve".into() }]);
            let suggestions = speller.suggestions(text, 10, 17, &english);
            assert_eq!(suggestions.first().map(String::as_str), Some("receive"), "{suggestions:?}");
            assert!(suggestions.len() <= 8, "{suggestions:?}");
            let found: Vec<String> = speller.check("Seperate the adress (Q3, 1st) from user@exampel.com: 日本語のtextも definately.", &english).into_iter().map(|m| m.word).collect();
            assert_eq!(found, ["Seperate", "adress", "definately"]);
            assert!(speller.suggestions("definately", 0, 10, &english).iter().any(|s| s == "definitely"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{char_offset, utf16_offset};

    #[test]
    fn offsets_between_characters_and_utf16() {
        // A character outside the BMP takes two UTF-16 units.
        let text = "a😀bc";
        assert_eq!(utf16_offset(text, 2), 3);
        assert_eq!(char_offset(text, 3), 2);
        assert_eq!(char_offset(text, 4), 3);
        assert_eq!(char_offset(text, 100), 4);
        assert_eq!(utf16_offset(text, usize::MAX), 5);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_misspelled_english_word_is_found() {
        let found = super::check("This is a recieve test", Some("en_US"));
        eprintln!("spell check found: {found:?}");
        // Where the system has an English dictionary.
        if let Some(first) = found.first() {
            assert_eq!((first.start, first.end, first.word.as_str()), (10, 17, "recieve"));
            assert!(super::suggestions("This is a recieve test", 10, 17, Some("en_US")).iter().any(|s| s == "receive"));
        }
    }
}
