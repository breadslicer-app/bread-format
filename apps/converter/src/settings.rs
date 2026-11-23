/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The choices of the window, kept for next time in a small text file of
//! `key=value` lines, in the user's configuration folder.

use std::fs;
use std::path::PathBuf;

/// What the window remembers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub direction: i32,
    pub subfolders: bool,
    pub output_mode: i32,
    pub output_folder: String,
    pub keep_tree: bool,
    pub when_there: i32,
    /// 0 convert and export, 1 convert only.
    pub export_mode: i32,
    /// Where the last file dialog was.
    pub last_folder: String,
    /// "en" or "es", or empty for the system's.
    pub language: String,
    /// The credit written into every `.bread` file: who designed the
    /// models, their license and where they come from. Empty for none.
    pub designer: String,
    pub license: String,
    pub source: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            direction: 0,
            subfolders: true,
            output_mode: 0,
            output_folder: String::new(),
            keep_tree: true,
            when_there: 0,
            export_mode: 0,
            last_folder: String::new(),
            language: String::new(),
            designer: String::new(),
            license: String::new(),
            source: String::new(),
        }
    }
}

impl Settings {
    /// Reads `text`, keeping the defaults for what it lacks or does not say
    /// well.
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let choice = |max: i32| {
                value
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|v| (0..=max).contains(v))
            };
            let flag = || match value.trim() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
            match key.trim() {
                "direction" => s.direction = choice(2).unwrap_or(s.direction),
                "subfolders" => s.subfolders = flag().unwrap_or(s.subfolders),
                "output_mode" => s.output_mode = choice(1).unwrap_or(s.output_mode),
                "output_folder" => s.output_folder = value.to_owned(),
                "keep_tree" => s.keep_tree = flag().unwrap_or(s.keep_tree),
                "when_there" => s.when_there = choice(2).unwrap_or(s.when_there),
                "export_mode" => s.export_mode = choice(1).unwrap_or(s.export_mode),
                "last_folder" => s.last_folder = value.to_owned(),
                "designer" => s.designer = value.to_owned(),
                "license" => s.license = value.to_owned(),
                "source" => s.source = value.to_owned(),
                "language" if matches!(value.trim(), "en" | "es") => {
                    s.language = value.trim().to_owned();
                }
                _ => {}
            }
        }
        s
    }

    pub fn text(&self) -> String {
        let one_line = |v: &str| v.replace(['\r', '\n'], "");
        format!(
            "direction={}\nsubfolders={}\noutput_mode={}\noutput_folder={}\nkeep_tree={}\nwhen_there={}\nexport_mode={}\nlast_folder={}\nlanguage={}\ndesigner={}\nlicense={}\nsource={}\n",
            self.direction,
            self.subfolders,
            self.output_mode,
            one_line(&self.output_folder),
            self.keep_tree,
            self.when_there,
            self.export_mode,
            one_line(&self.last_folder),
            self.language,
            one_line(&self.designer),
            one_line(&self.license),
            one_line(&self.source),
        )
    }

    /// The settings kept, or the defaults.
    pub fn load() -> Self {
        file()
            .and_then(|f| fs::read_to_string(f).ok())
            .map_or_else(Self::default, |t| Self::parse(&t))
    }

    /// Keeps them; a failure only loses them.
    pub fn save(&self) {
        if let Some(f) = file() {
            if let Some(dir) = f.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(f, self.text());
        }
    }
}

/// The settings file: in `%APPDATA%` on Windows, `~/Library/Application
/// Support` on macOS, and `$XDG_CONFIG_HOME` or `~/.config` elsewhere.
fn file() -> Option<PathBuf> {
    let var = |k| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")?
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))?
    };
    Some(base.join("Bread Converter").join("settings.txt"))
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn settings_come_back_as_they_were_kept() {
        let s = Settings {
            direction: 2,
            subfolders: false,
            output_mode: 1,
            output_folder: "D:/out folder".into(),
            keep_tree: false,
            when_there: 2,
            export_mode: 1,
            last_folder: "C:/models".into(),
            language: "es".into(),
            designer: "A. Designer = me".into(),
            license: "CC-BY-4.0".into(),
            source: "https://example.com/models?a=b".into(),
        };
        assert_eq!(Settings::parse(&s.text()), s);
    }

    #[test]
    fn what_is_not_understood_keeps_the_default() {
        let s = Settings::parse("direction=7\nsubfolders=maybe\nnonsense\nwhen_there=1\n");
        assert_eq!(s.direction, 0);
        assert!(s.subfolders);
        assert_eq!(s.when_there, 1);
    }
}
