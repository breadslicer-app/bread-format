/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The role chunk (`SPEC.md`, section 11): which leaves are not models, but
//! supports or a raft.
//!
//! Each entry names a leaf, and no two name the same one. Reserved roles are
//! carried as they are: a program treats them as absent, so their leaves are
//! models to it. How roles agree with the supports and hollows chunks is
//! checked where the whole file is read and written.
//!
//! ```text
//! entries   u32 count, then per entry:
//!           u32 the leaf's node index in the scene chunk
//!           u8  the role: 1 supports, 2 a raft
//! ```

use std::collections::HashSet;
use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::scene::Scene;

/// The tag of the role chunk.
pub const ROLE_TAG: ChunkTag = ChunkTag(*b"ROLE");

/// The role payload layout this build writes and reads.
pub const ROLE_VERSION: u16 = 1;

/// The role of a leaf that shows supports.
pub const ROLE_SUPPORTS: u8 = 1;

/// The role of a leaf that shows a raft.
pub const ROLE_RAFT: u8 = 2;

/// The role of one leaf that is not a model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoleEntry {
    /// The leaf, as an index into the scene's nodes.
    pub leaf: usize,
    /// What the leaf is: [`ROLE_SUPPORTS`] or [`ROLE_RAFT`]. Other values are
    /// reserved, carried as they are, and mean a model.
    pub role: u8,
}

impl RoleEntry {
    /// Whether the role is one this version of the format names, and not a
    /// reserved one.
    pub fn is_known(&self) -> bool {
        matches!(self.role, ROLE_SUPPORTS | ROLE_RAFT)
    }
}

/// Why a list of roles could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeRolesError {
    /// An entry names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// Two entries name the same leaf.
    SameLeaf,
    /// An entry has a reserved role, which a writer must not write.
    ReservedRole,
    /// There are 2^32 or more entries.
    TooLarge,
}

impl fmt::Display for EncodeRolesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::SameLeaf => f.write_str(SAME_LEAF),
            Self::ReservedRole => f.write_str("a role entry has a reserved role"),
            Self::TooLarge => f.write_str("the roles are too many for their chunk"),
        }
    }
}

impl std::error::Error for EncodeRolesError {}

/// Why a role payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeRolesError {
    /// The payload is cut short or padded.
    Malformed,
    /// An entry names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// Two entries name the same leaf.
    SameLeaf,
}

impl fmt::Display for DecodeRolesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("the role data is malformed"),
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::SameLeaf => f.write_str(SAME_LEAF),
        }
    }
}

impl std::error::Error for DecodeRolesError {}

const NOT_A_LEAF: &str = "a role names a scene part that is not a leaf";
const SAME_LEAF: &str = "two role entries name the same leaf";

/// A rule of section 11 that some entries break.
enum Broken {
    NotALeaf,
    SameLeaf,
}

impl From<Broken> for EncodeRolesError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::SameLeaf => Self::SameLeaf,
        }
    }
}

impl From<Broken> for DecodeRolesError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::SameLeaf => Self::SameLeaf,
        }
    }
}

/// Encodes `entries` as a role payload, in their order, checking them
/// against `scene`.
pub fn encode_roles(entries: &[RoleEntry], scene: &Scene) -> Result<Vec<u8>, EncodeRolesError> {
    check(entries, scene)?;
    if entries.iter().any(|entry| !entry.is_known()) {
        return Err(EncodeRolesError::ReservedRole);
    }
    if !fits_u32(entries.len()) {
        return Err(EncodeRolesError::TooLarge);
    }
    let mut out = Out::default();
    out.len(entries.len());
    for entry in entries {
        // The leaf was checked against the scene, whose node count fits.
        out.len(entry.leaf);
        out.u8(entry.role);
    }
    Ok(out.0)
}

/// Decodes a role payload, checking it against `scene`, the scene of the
/// same file.
pub fn decode_roles(payload: &[u8], scene: &Scene) -> Result<Vec<RoleEntry>, DecodeRolesError> {
    let mut input = In(payload);
    let malformed = DecodeRolesError::Malformed;
    let count = input.count(5).ok_or(malformed.clone())?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let leaf = input.len().ok_or(malformed.clone())?;
        let role = input.u8().ok_or(malformed.clone())?;
        entries.push(RoleEntry { leaf, role });
    }
    if !input.is_empty() {
        return Err(malformed);
    }
    check(&entries, scene)?;
    Ok(entries)
}

/// Every entry names a leaf, and no two name the same one.
fn check(entries: &[RoleEntry], scene: &Scene) -> Result<(), Broken> {
    let mut seen = HashSet::new();
    for entry in entries {
        scene.leaf_mesh(entry.leaf).ok_or(Broken::NotALeaf)?;
        if !seen.insert(entry.leaf) {
            return Err(Broken::SameLeaf);
        }
    }
    Ok(())
}
