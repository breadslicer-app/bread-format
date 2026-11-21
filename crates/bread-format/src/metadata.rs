/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The metadata chunk (`SPEC.md`, section 12): who made a file, how it may be
//! used, and lasting ids for its parts, as entries of a subject, a key and a
//! value.
//!
//! Metadata never makes a file unreadable. Decoding drops what breaks a rule,
//! entry by entry, or the whole payload when it is malformed; encoding refuses
//! what a writer must not write.
//!
//! ```text
//! entries   u32 count, then per entry:
//!           u32 the node index the entry is about, or FFFFFFFF for the file
//!           u32 length, key
//!           u32 length, value
//! ```

use std::collections::HashSet;
use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::scene::Scene;

/// The tag of the metadata chunk.
pub const METADATA_TAG: ChunkTag = ChunkTag(*b"META");

/// The metadata payload layout this build writes and reads.
pub const METADATA_VERSION: u16 = 1;

/// The registered keys whose value is UTF-8 text (`SPEC.md`, 12.1).
pub const TEXT_KEYS: [&str; 8] = [
    "title",
    "description",
    "designer",
    "copyright",
    "license",
    "source",
    "created",
    "generator",
];

/// The registered key whose value is the 16 bytes of a UUID.
pub const UUID_KEY: &str = "uuid";

/// The subject that stands for the whole file in a payload.
const FILE: u32 = u32::MAX;

/// The longest key, in bytes.
const MAX_KEY: usize = 64;

/// One entry of metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataEntry {
    /// The node the entry is about, as an index into the scene's nodes, or
    /// `None` for the whole file.
    pub node: Option<usize>,
    /// What the value is: a registered key, such as `title`, or a program's
    /// own, with a dot, such as `breadslicer.printer`.
    pub key: String,
    /// The value: UTF-8 text for the registered text keys, 16 bytes for
    /// `uuid`, and whatever a program makes of it for its own keys.
    pub value: Vec<u8>,
}

impl MetadataEntry {
    /// An entry with a text value, such as a title.
    pub fn text(node: Option<usize>, key: &str, text: &str) -> Self {
        Self {
            node,
            key: key.into(),
            value: text.as_bytes().to_vec(),
        }
    }

    /// An entry with a UUID, given as its 16 bytes in the order of RFC 9562.
    pub fn uuid(node: Option<usize>, uuid: [u8; 16]) -> Self {
        Self {
            node,
            key: UUID_KEY.into(),
            value: uuid.to_vec(),
        }
    }

    /// The value as text, when the key is a registered text key.
    pub fn as_text(&self) -> Option<&str> {
        match key_kind(self.key.as_bytes()) {
            KeyKind::Text => std::str::from_utf8(&self.value).ok(),
            _ => None,
        }
    }

    /// The value as a UUID, when the key is `uuid`.
    pub fn as_uuid(&self) -> Option<[u8; 16]> {
        match key_kind(self.key.as_bytes()) {
            KeyKind::Uuid => self.value.as_slice().try_into().ok(),
            _ => None,
        }
    }
}

/// Why a list of metadata entries could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeMetadataError {
    /// An entry names a node the scene does not have.
    NoSuchNode { node: usize },
    /// A key is not 1 to 64 lowercase letters, digits, dots, hyphens and
    /// underscores, starting with a letter.
    BadKey { key: String },
    /// A key without a dot that the format does not register.
    ReservedKey { key: String },
    /// A registered key's value is not what the key takes: UTF-8 text, or
    /// 16 bytes for `uuid`.
    BadValue { key: String },
    /// Two entries have the same subject and key.
    SameEntry { key: String },
    /// There are 2^32 or more entries, or a value of 4 GiB or more.
    TooLarge,
}

impl fmt::Display for EncodeMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchNode { node } => write!(
                f,
                "metadata names node {node}, which the scene does not have"
            ),
            Self::BadKey { key } => write!(f, "{key:?} is not a metadata key"),
            Self::ReservedKey { key } => write!(f, "the metadata key {key:?} is reserved"),
            Self::BadValue { key } => write!(
                f,
                "the value of the metadata key {key:?} is not what it takes"
            ),
            Self::SameEntry { key } => write!(
                f,
                "two metadata entries have the same subject and the key {key:?}"
            ),
            Self::TooLarge => f.write_str("the metadata is too large for its chunk"),
        }
    }
}

impl std::error::Error for EncodeMetadataError {}

/// What a key is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    /// A registered key with a text value.
    Text,
    /// `uuid`.
    Uuid,
    /// A program's own key: it has a dot.
    Program,
    /// A key without a dot that is not registered.
    Reserved,
    /// Not a key at all.
    Invalid,
}

fn key_kind(key: &[u8]) -> KeyKind {
    let well_formed = (1..=MAX_KEY).contains(&key.len())
        && key[0].is_ascii_lowercase()
        && key
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b));
    if !well_formed {
        return KeyKind::Invalid;
    }
    if key.contains(&b'.') {
        return KeyKind::Program;
    }
    if key == UUID_KEY.as_bytes() {
        return KeyKind::Uuid;
    }
    if TEXT_KEYS.iter().any(|text| text.as_bytes() == key) {
        return KeyKind::Text;
    }
    KeyKind::Reserved
}

/// Whether `value` is what a key of `kind` takes.
fn fits(kind: KeyKind, value: &[u8]) -> bool {
    match kind {
        KeyKind::Text => std::str::from_utf8(value).is_ok(),
        KeyKind::Uuid => value.len() == 16,
        KeyKind::Program => true,
        KeyKind::Reserved | KeyKind::Invalid => false,
    }
}

/// Encodes `entries` as a metadata payload, in their order, checking them
/// against `scene`.
pub fn encode_metadata(
    entries: &[MetadataEntry],
    scene: &Scene,
) -> Result<Vec<u8>, EncodeMetadataError> {
    let mut seen = HashSet::new();
    for entry in entries {
        if let Some(node) = entry.node
            && node >= scene.nodes.len()
        {
            return Err(EncodeMetadataError::NoSuchNode { node });
        }
        let key = entry.key.clone();
        match key_kind(entry.key.as_bytes()) {
            KeyKind::Invalid => return Err(EncodeMetadataError::BadKey { key }),
            KeyKind::Reserved => return Err(EncodeMetadataError::ReservedKey { key }),
            kind if !fits(kind, &entry.value) => {
                return Err(EncodeMetadataError::BadValue { key });
            }
            _ => {}
        }
        if !seen.insert((entry.node, entry.key.as_str())) {
            return Err(EncodeMetadataError::SameEntry { key });
        }
    }
    if !fits_u32(entries.len()) || entries.iter().any(|entry| !fits_u32(entry.value.len())) {
        return Err(EncodeMetadataError::TooLarge);
    }
    let mut out = Out::default();
    out.len(entries.len());
    for entry in entries {
        // A node was checked against the scene, whose node count fits.
        out.u32(entry.node.map_or(FILE, |node| node as u32));
        out.bytes(entry.key.as_bytes());
        out.bytes(&entry.value);
    }
    Ok(out.0)
}

/// Decodes a metadata payload against `scene`, the scene of the same file.
/// It never fails: a malformed payload is no metadata at all, and an entry
/// that breaks a rule of section 12 is left out (`SPEC.md`, 12.2).
pub fn decode_metadata(payload: &[u8], scene: &Scene) -> Vec<MetadataEntry> {
    let Some(raw) = split(payload) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for (subject, key, value) in raw {
        // The second of two entries with the same subject and key is left
        // out, whatever the first holds.
        if !seen.insert((subject, key)) {
            continue;
        }
        let node = match subject {
            FILE => None,
            node if (node as usize) < scene.nodes.len() => Some(node as usize),
            _ => continue,
        };
        let kind = key_kind(key);
        if !fits(kind, value) {
            continue;
        }
        entries.push(MetadataEntry {
            node,
            // A key of any kind but `Invalid` is ASCII.
            key: String::from_utf8_lossy(key).into_owned(),
            value: value.to_vec(),
        });
    }
    entries
}

/// An entry as stored: its subject, key and value.
type RawEntry<'a> = (u32, &'a [u8], &'a [u8]);

/// The entries of a payload as stored, or `None` when it is malformed.
fn split(payload: &[u8]) -> Option<Vec<RawEntry<'_>>> {
    let mut input = In(payload);
    let count = input.count(12)?;
    let mut raw = Vec::with_capacity(count);
    for _ in 0..count {
        let subject = input.u32()?;
        let key = input.bytes()?;
        let value = input.bytes()?;
        raw.push((subject, key, value));
    }
    input.is_empty().then_some(raw)
}
