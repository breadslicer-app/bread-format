/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The surface paint chunk (`SPEC.md`, section 10): what a person painted on
//! the faces of models, one mask per leaf and kind.
//!
//! Only the envelope is checked: each entry names a leaf, and no two entries
//! share a leaf and a kind. Documents are carried without being read, since a
//! document never makes a file unreadable; the program that uses the paint
//! checks it against the leaf's mesh.
//!
//! ```text
//! entries   u32 count, then per entry:
//!           u32 the leaf's node index in the scene chunk
//!           u8  the kind: 0 detail, 1 supports must touch, 2 must not touch
//!           u32 document version
//!           u32 length, document
//! ```

use std::collections::HashSet;
use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::scene::Scene;

/// The tag of the surface paint chunk.
pub const SURFACE_TAG: ChunkTag = ChunkTag(*b"SURF");

/// The surface paint payload layout this build writes and reads.
pub const SURFACE_VERSION: u16 = 1;

/// One painted mask of one leaf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceEntry {
    /// The painted leaf, as an index into the scene's nodes.
    pub leaf: usize,
    /// What the paint means: 0 detail, 1 supports must touch, 2 supports
    /// must not touch. Other values are reserved, and carried as they are.
    pub kind: u8,
    /// The version of the document.
    pub version: u32,
    /// The mask, carried without being read.
    pub document: Vec<u8>,
}

/// Why a list of paint entries could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeSurfaceError {
    /// An entry names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// Two entries have the same leaf and kind.
    SameEntry,
    /// There are 2^32 or more entries, or a document of 4 GiB or more.
    TooLarge,
}

impl fmt::Display for EncodeSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::SameEntry => f.write_str(SAME_ENTRY),
            Self::TooLarge => f.write_str("the surface paint is too large for its chunk"),
        }
    }
}

impl std::error::Error for EncodeSurfaceError {}

/// Why a surface paint payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeSurfaceError {
    /// The payload is cut short or padded.
    Malformed,
    /// An entry names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// Two entries have the same leaf and kind.
    SameEntry,
}

impl fmt::Display for DecodeSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("the surface paint data is malformed"),
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::SameEntry => f.write_str(SAME_ENTRY),
        }
    }
}

impl std::error::Error for DecodeSurfaceError {}

const NOT_A_LEAF: &str = "surface paint names a scene part that is not a leaf";
const SAME_ENTRY: &str = "two surface paint entries have the same leaf and kind";

/// A rule of section 10 that some entries break.
enum Broken {
    NotALeaf,
    SameEntry,
}

impl From<Broken> for EncodeSurfaceError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::SameEntry => Self::SameEntry,
        }
    }
}

impl From<Broken> for DecodeSurfaceError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::SameEntry => Self::SameEntry,
        }
    }
}

/// Encodes `entries` as a surface paint payload, in their order, checking
/// them against `scene`.
pub fn encode_surface(
    entries: &[SurfaceEntry],
    scene: &Scene,
) -> Result<Vec<u8>, EncodeSurfaceError> {
    check(entries, scene)?;
    if !fits_u32(entries.len()) || entries.iter().any(|entry| !fits_u32(entry.document.len())) {
        return Err(EncodeSurfaceError::TooLarge);
    }
    let mut out = Out::default();
    out.len(entries.len());
    for entry in entries {
        // The leaf was checked against the scene, whose node count fits.
        out.len(entry.leaf);
        out.u8(entry.kind);
        out.u32(entry.version);
        out.bytes(&entry.document);
    }
    Ok(out.0)
}

/// Decodes a surface paint payload, checking it against `scene`, the scene
/// of the same file.
pub fn decode_surface(
    payload: &[u8],
    scene: &Scene,
) -> Result<Vec<SurfaceEntry>, DecodeSurfaceError> {
    let mut input = In(payload);
    let malformed = DecodeSurfaceError::Malformed;
    let count = input.count(13).ok_or(malformed.clone())?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let leaf = input.len().ok_or(malformed.clone())?;
        let kind = input.u8().ok_or(malformed.clone())?;
        let version = input.u32().ok_or(malformed.clone())?;
        let document = input.bytes().ok_or(malformed.clone())?.to_vec();
        entries.push(SurfaceEntry {
            leaf,
            kind,
            version,
            document,
        });
    }
    if !input.is_empty() {
        return Err(malformed);
    }
    check(&entries, scene)?;
    Ok(entries)
}

/// Every entry names a leaf, and no two share a leaf and a kind.
fn check(entries: &[SurfaceEntry], scene: &Scene) -> Result<(), Broken> {
    let mut seen = HashSet::new();
    for entry in entries {
        scene.leaf_mesh(entry.leaf).ok_or(Broken::NotALeaf)?;
        if !seen.insert((entry.leaf, entry.kind)) {
            return Err(Broken::SameEntry);
        }
    }
    Ok(())
}
