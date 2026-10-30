/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The support preview chunk (`SPEC.md`, section 9): for support leaves,
//! which are stored with an empty mesh, the mesh of the supports as they were
//! when the file was written, so that any reader can show them.
//!
//! Each mesh is a whole mesh chunk payload, so it is checked, and kept, like
//! any other mesh.
//!
//! ```text
//! previews  u32 count, then per preview:
//!           u32 the support leaf's node index in the scene chunk
//!           u32 length, mesh chunk payload (version 1)
//! ```

use std::collections::HashSet;
use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::mesh::{DecodeMeshError, EncodeMeshError, Mesh, decode_mesh, encode_mesh};
use crate::scene::Scene;

/// The tag of the support preview chunk.
pub const SUPPORT_PREVIEW_TAG: ChunkTag = ChunkTag(*b"SPRV");

/// The support preview payload layout this build writes and reads. It holds
/// meshes of mesh chunk version 1.
pub const SUPPORT_PREVIEW_VERSION: u16 = 1;

/// What a reader that skips the supports chunk can show for one support leaf.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SupportPreview {
    /// The support leaf, as an index into the scene's nodes.
    pub leaf: usize,
    /// The supports, in the leaf's own space.
    pub mesh: Mesh,
}

/// Why a list of support previews could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodePreviewsError {
    /// A preview names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// A preview names a leaf that shows a mesh with vertices or triangles.
    MeshNotEmpty,
    /// Two previews name the same leaf.
    SameLeaf,
    Mesh(EncodeMeshError),
    /// There are 2^32 or more previews, or a mesh encodes to 4 GiB or more.
    TooLarge,
}

impl fmt::Display for EncodePreviewsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::MeshNotEmpty => f.write_str(MESH_NOT_EMPTY),
            Self::SameLeaf => f.write_str(SAME_LEAF),
            Self::Mesh(err) => write!(f, "{err}"),
            Self::TooLarge => f.write_str("the previews are too large for their chunk"),
        }
    }
}

impl std::error::Error for EncodePreviewsError {}

/// Why a support preview payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodePreviewsError {
    /// The payload is cut short or padded.
    Malformed,
    /// A preview's mesh is not a valid mesh chunk payload.
    Mesh(DecodeMeshError),
    /// A preview names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
    /// A preview names a leaf that shows a mesh with vertices or triangles.
    MeshNotEmpty,
    /// Two previews name the same leaf.
    SameLeaf,
}

impl fmt::Display for DecodePreviewsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("the support preview data is malformed"),
            Self::Mesh(err) => write!(f, "{err}"),
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::MeshNotEmpty => f.write_str(MESH_NOT_EMPTY),
            Self::SameLeaf => f.write_str(SAME_LEAF),
        }
    }
}

impl std::error::Error for DecodePreviewsError {}

const NOT_A_LEAF: &str = "a support preview names a scene part that is not a leaf";
const MESH_NOT_EMPTY: &str = "a support preview names a leaf that shows a mesh of its own";
const SAME_LEAF: &str = "two support previews name the same leaf";

/// A rule of section 9 that some previews break.
enum Broken {
    NotALeaf,
    MeshNotEmpty,
    SameLeaf,
}

impl From<Broken> for EncodePreviewsError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::MeshNotEmpty => Self::MeshNotEmpty,
            Broken::SameLeaf => Self::SameLeaf,
        }
    }
}

impl From<Broken> for DecodePreviewsError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::MeshNotEmpty => Self::MeshNotEmpty,
            Broken::SameLeaf => Self::SameLeaf,
        }
    }
}

/// Encodes `previews` as a support preview payload, checking them against
/// `scene`.
pub fn encode_previews(
    previews: &[SupportPreview],
    scene: &Scene,
) -> Result<Vec<u8>, EncodePreviewsError> {
    check(previews, scene)?;
    if !fits_u32(previews.len()) {
        return Err(EncodePreviewsError::TooLarge);
    }
    let mut out = Out::default();
    out.len(previews.len());
    for preview in previews {
        let mesh = encode_mesh(&preview.mesh).map_err(EncodePreviewsError::Mesh)?;
        if !fits_u32(mesh.len()) {
            return Err(EncodePreviewsError::TooLarge);
        }
        // The leaf was checked against the scene, whose node count fits.
        out.len(preview.leaf);
        out.bytes(&mesh);
    }
    Ok(out.0)
}

/// Decodes a support preview payload, checking it against `scene`, the scene
/// of the same file.
pub fn decode_previews(
    payload: &[u8],
    scene: &Scene,
) -> Result<Vec<SupportPreview>, DecodePreviewsError> {
    let mut input = In(payload);
    let malformed = DecodePreviewsError::Malformed;
    let count = input.count(8).ok_or(malformed.clone())?;
    let mut previews = Vec::with_capacity(count);
    for _ in 0..count {
        let leaf = input.len().ok_or(malformed.clone())?;
        let mesh = input.bytes().ok_or(malformed.clone())?;
        let mesh = decode_mesh(mesh).map_err(DecodePreviewsError::Mesh)?;
        previews.push(SupportPreview { leaf, mesh });
    }
    if !input.is_empty() {
        return Err(malformed);
    }
    check(&previews, scene)?;
    Ok(previews)
}

/// Every preview names a leaf with an empty mesh, and no two the same one.
fn check(previews: &[SupportPreview], scene: &Scene) -> Result<(), Broken> {
    let mut leaves = HashSet::new();
    for preview in previews {
        let mesh = scene.leaf_mesh(preview.leaf).ok_or(Broken::NotALeaf)?;
        if !mesh.vertices.is_empty() || !mesh.triangles.is_empty() {
            return Err(Broken::MeshNotEmpty);
        }
        if !leaves.insert(preview.leaf) {
            return Err(Broken::SameLeaf);
        }
    }
    Ok(())
}
