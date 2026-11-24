/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Who made a model and how it may be used, written as metadata about the
//! whole file (`SPEC.md` 12), with the program that wrote it and when.

use std::time::{SystemTime, UNIX_EPOCH};

use bread_format::MetadataEntry;

use crate::Error;

/// The credit a converted file carries: each field is written when it is
/// there, as the registered key of its name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credit {
    pub title: Option<String>,
    pub designer: Option<String>,
    /// An SPDX license expression, such as `CC-BY-4.0`; any text is kept.
    pub license: Option<String>,
    /// Where the model comes from, as a URL.
    pub source: Option<String>,
}

impl Credit {
    /// Each field without the spaces around it, and those left empty gone.
    #[must_use]
    pub fn tidy(self) -> Self {
        let tidy =
            |field: Option<String>| field.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        Self {
            title: tidy(self.title),
            designer: tidy(self.designer),
            license: tidy(self.license),
            source: tidy(self.source),
        }
    }

    /// Whether every field can be written as `SPEC.md` 12.1 asks: each one
    /// line of text, and the source a URL. Says which cannot, and why.
    pub fn check(&self) -> Result<(), Error> {
        for (what, value) in self.fields() {
            if value.chars().any(char::is_control) {
                return Err(Error(format!(
                    "the {what} is not one line of text: it has a line break or another control character"
                )));
            }
        }
        if let Some(source) = &self.source
            && !is_url(source)
        {
            return Err(Error(format!(
                "the source {source:?} is not a web address: give it whole, such as https://example.com/model"
            )));
        }
        Ok(())
    }

    /// The metadata entries about the whole file: the credit's fields, then
    /// `created` and `generator`.
    pub fn metadata(&self, created: &str, generator: &str) -> Vec<MetadataEntry> {
        self.fields()
            .into_iter()
            .chain([("created", created), ("generator", generator)])
            .map(|(key, value)| MetadataEntry::text(None, key, value))
            .collect()
    }

    /// The fields that are there, by their keys, in the order written.
    fn fields(&self) -> Vec<(&'static str, &str)> {
        [
            ("title", &self.title),
            ("designer", &self.designer),
            ("license", &self.license),
            ("source", &self.source),
        ]
        .into_iter()
        .filter_map(|(key, value)| Some((key, value.as_deref()?)))
        .collect()
    }
}

/// Whether `text` reads as a URL: a scheme, such as `https`, a colon and
/// more, with no spaces (RFC 3986).
fn is_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut letters = scheme.chars();
    letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !rest.strip_prefix("//").unwrap_or(rest).is_empty()
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Now, in UTC to the second, as `created` takes it (RFC 3339), such as
/// `2026-11-23T14:30:00Z`. When `SOURCE_DATE_EPOCH` is set to a number of
/// seconds, that time instead, so that a conversion can be made again byte
/// for byte.
pub fn now() -> String {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
    rfc3339(seconds)
}

/// Seconds since 1970 in UTC, as RFC 3339.
fn rfc3339(seconds: u64) -> String {
    let days = seconds / 86_400;
    let of_day = seconds % 86_400;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day / 60 % 60,
        of_day % 60
    )
}

/// The date `days` after 1970-01-01, in the proleptic Gregorian calendar
/// (H. Hinnant's `civil_from_days`, for days on or after 1970).
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use bread_format::MetadataEntry;

    use super::{Credit, is_url, rfc3339};

    fn credit() -> Credit {
        Credit {
            title: Some("Dragon".into()),
            designer: Some("A. Designer".into()),
            license: Some("CC-BY-4.0".into()),
            source: Some("https://example.com/dragon".into()),
        }
    }

    #[test]
    fn the_credit_becomes_entries_about_the_file_in_order() {
        let entries = credit().metadata("2026-11-23T14:30:00Z", "bread 0.1.0");
        let keys: Vec<_> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "title",
                "designer",
                "license",
                "source",
                "created",
                "generator"
            ]
        );
        assert!(entries.iter().all(|e| e.node.is_none()));
        assert_eq!(
            entries[1],
            MetadataEntry::text(None, "designer", "A. Designer")
        );
        let none = Credit::default().metadata("2026-11-23T14:30:00Z", "bread 0.1.0");
        assert_eq!(none.len(), 2);
    }

    #[test]
    fn tidy_trims_and_drops_what_is_empty() {
        let tidy = Credit {
            title: Some("  Dragon \t".into()),
            designer: Some("   ".into()),
            license: None,
            source: Some(String::new()),
        }
        .tidy();
        assert_eq!(
            tidy,
            Credit {
                title: Some("Dragon".into()),
                ..Credit::default()
            }
        );
    }

    #[test]
    fn check_refuses_what_cannot_be_written() {
        assert!(credit().check().is_ok());
        assert!(Credit::default().check().is_ok());
        let two_lines = Credit {
            designer: Some("A.\nDesigner".into()),
            ..credit()
        };
        assert!(two_lines.check().unwrap_err().0.contains("designer"));
        let no_scheme = Credit {
            source: Some("example.com/dragon".into()),
            ..credit()
        };
        assert!(no_scheme.check().unwrap_err().0.contains("source"));
    }

    #[test]
    fn urls_need_a_scheme_and_no_spaces() {
        for good in [
            "https://example.com/m",
            "http://x.y",
            "mailto:a@b.c",
            "ipfs://bafy",
        ] {
            assert!(is_url(good), "{good}");
        }
        for bad in [
            "example.com",
            "https://",
            "://x",
            "1http://x",
            "https://a b",
            "",
        ] {
            assert!(!is_url(bad), "{bad}");
        }
    }

    #[test]
    fn times_are_rfc_3339_in_utc() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_795_444_200), "2026-11-23T14:30:00Z");
        assert_eq!(rfc3339(4_102_444_799), "2099-12-31T23:59:59Z");
    }
}
