/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The application chunks (`SPEC.md`, section 7.3): documents that belong to
//! one program, carried without being read.
//!
//! The project settings chunk is the settings document itself. The supports
//! and hollows chunks share one envelope: a list of entries, each about one
//! model of the scene, naming one more part of it, and holding a document
//! named by the generator that wrote it. In the supports chunk that part is
//! the leaf that shows the supports; in the hollows chunk it is the mesh the
//! model was hollowed from. The crate checks the envelope against the scene
//! and never reads the documents: telling which generators and document
//! versions it understands is up to the program.
//!
//! ```text
//! entries   u32 count, then per entry:
//!           u32 the model leaf's node index in the scene chunk
//!           u32 supports: the support leaf's node index
//!               hollows: the original's mesh index
//!           u32 length, UTF-8 generator name
//!           u32 generator document version
//!           u32 length, document bytes
//! ```

use std::collections::HashSet;
use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::scene::{NodeKind, Scene};

/// The tag of the project settings chunk, whose payload is the settings
/// document: UTF-8 TOML that starts with its own version.
pub const SETTINGS_TAG: ChunkTag = ChunkTag(*b"PROJ");

/// The project settings chunk version this build writes and reads.
pub const SETTINGS_VERSION: u16 = 1;

/// The tag of the supports chunk. Its support leaves are stored with an
/// empty mesh, which the generator rebuilds from the document.
pub const SUPPORTS_TAG: ChunkTag = ChunkTag(*b"SUPP");

/// The supports payload layout this build writes and reads.
pub const SUPPORTS_VERSION: u16 = 2;

/// The tag of the hollows chunk. Its models show the hollow.
pub const HOLLOWS_TAG: ChunkTag = ChunkTag(*b"HOLW");

/// The hollows payload layout this build writes and reads.
pub const HOLLOWS_VERSION: u16 = 2;

/// A program's supports for one model of the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportEntry {
    /// The leaf of the model the supports stand on, as an index into the
    /// scene's nodes.
    pub model: usize,
    /// The leaf that shows the supports, with an empty mesh.
    pub support: usize,
    /// Which generator wrote `document`, such as `"df"`.
    pub generator: String,
    /// The generator's own version of `document`.
    pub version: u32,
    pub document: Vec<u8>,
}

/// How a program hollowed one model of the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HollowEntry {
    /// The leaf of the hollowed model, as an index into the scene's nodes.
    pub model: usize,
    /// The mesh the model was hollowed from, as an index into the scene's
    /// meshes.
    pub original: usize,
    /// Which generator wrote `document`, such as `"df"`.
    pub generator: String,
    /// The generator's own version of `document`.
    pub version: u32,
    pub document: Vec<u8>,
}

/// Why a list of supports or hollows could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeDocumentsError {
    /// A model or a support leaf names a node the scene does not have, or
    /// one that is not a leaf.
    NotALeaf,
    /// An original names a mesh the scene does not have.
    NoSuchMesh,
    /// Two entries name the same model.
    SameModel,
    /// Two entries name the same support leaf, or a support leaf is also a
    /// model.
    SharedSupport,
    /// There are 2^32 or more entries, or a generator name or a document of
    /// 4 GiB or more.
    TooLarge,
}

impl fmt::Display for EncodeDocumentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::NoSuchMesh => f.write_str(NO_SUCH_MESH),
            Self::SameModel => f.write_str(SAME_MODEL),
            Self::SharedSupport => f.write_str(SHARED_SUPPORT),
            Self::TooLarge => f.write_str("the documents are too large for their chunk"),
        }
    }
}

impl std::error::Error for EncodeDocumentsError {}

/// Why a supports or hollows payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeDocumentsError {
    /// The payload is cut short, padded, or not text where text belongs.
    Malformed,
    /// A model or a support leaf names a node the scene does not have, or
    /// one that is not a leaf.
    NotALeaf,
    /// An original names a mesh the scene does not have.
    NoSuchMesh,
    /// Two entries name the same model.
    SameModel,
    /// Two entries name the same support leaf, or a support leaf is also a
    /// model.
    SharedSupport,
}

impl fmt::Display for DecodeDocumentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("the document data is malformed"),
            Self::NotALeaf => f.write_str(NOT_A_LEAF),
            Self::NoSuchMesh => f.write_str(NO_SUCH_MESH),
            Self::SameModel => f.write_str(SAME_MODEL),
            Self::SharedSupport => f.write_str(SHARED_SUPPORT),
        }
    }
}

impl std::error::Error for DecodeDocumentsError {}

const NOT_A_LEAF: &str = "a document names a scene part that is not a model";
const NO_SUCH_MESH: &str = "a hollow names an original mesh the file does not have";
const SAME_MODEL: &str = "two documents of one kind belong to the same model";
const SHARED_SUPPORT: &str = "a support part belongs to two models, or is a model itself";

/// A rule of section 7.3 that some entries break.
enum Broken {
    NotALeaf,
    NoSuchMesh,
    SameModel,
    SharedSupport,
}

impl From<Broken> for EncodeDocumentsError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::NoSuchMesh => Self::NoSuchMesh,
            Broken::SameModel => Self::SameModel,
            Broken::SharedSupport => Self::SharedSupport,
        }
    }
}

impl From<Broken> for DecodeDocumentsError {
    fn from(broken: Broken) -> Self {
        match broken {
            Broken::NotALeaf => Self::NotALeaf,
            Broken::NoSuchMesh => Self::NoSuchMesh,
            Broken::SameModel => Self::SameModel,
            Broken::SharedSupport => Self::SharedSupport,
        }
    }
}

/// The fields every entry has, in the order of the payload.
struct Entry<'a> {
    model: usize,
    part: usize,
    generator: &'a str,
    version: u32,
    document: &'a [u8],
}

/// Encodes `supports` as a supports payload, checking them against `scene`.
pub fn encode_supports(
    supports: &[SupportEntry],
    scene: &Scene,
) -> Result<Vec<u8>, EncodeDocumentsError> {
    check_supports(supports, scene)?;
    encode(supports.iter().map(|entry| Entry {
        model: entry.model,
        part: entry.support,
        generator: &entry.generator,
        version: entry.version,
        document: &entry.document,
    }))
}

/// Encodes `hollows` as a hollows payload, checking them against `scene`.
pub fn encode_hollows(
    hollows: &[HollowEntry],
    scene: &Scene,
) -> Result<Vec<u8>, EncodeDocumentsError> {
    check_hollows(hollows, scene)?;
    encode(hollows.iter().map(|entry| Entry {
        model: entry.model,
        part: entry.original,
        generator: &entry.generator,
        version: entry.version,
        document: &entry.document,
    }))
}

/// Decodes a supports payload, checking it against `scene`, the scene of the
/// same file.
pub fn decode_supports(
    payload: &[u8],
    scene: &Scene,
) -> Result<Vec<SupportEntry>, DecodeDocumentsError> {
    let supports: Vec<_> = decode(payload)?
        .into_iter()
        .map(|entry| SupportEntry {
            model: entry.model,
            support: entry.part,
            generator: entry.generator.to_owned(),
            version: entry.version,
            document: entry.document.to_vec(),
        })
        .collect();
    check_supports(&supports, scene)?;
    Ok(supports)
}

/// Decodes a hollows payload, checking it against `scene`, the scene of the
/// same file.
pub fn decode_hollows(
    payload: &[u8],
    scene: &Scene,
) -> Result<Vec<HollowEntry>, DecodeDocumentsError> {
    let hollows: Vec<_> = decode(payload)?
        .into_iter()
        .map(|entry| HollowEntry {
            model: entry.model,
            original: entry.part,
            generator: entry.generator.to_owned(),
            version: entry.version,
            document: entry.document.to_vec(),
        })
        .collect();
    check_hollows(&hollows, scene)?;
    Ok(hollows)
}

fn encode<'a>(
    entries: impl ExactSizeIterator<Item = Entry<'a>>,
) -> Result<Vec<u8>, EncodeDocumentsError> {
    if !fits_u32(entries.len()) {
        return Err(EncodeDocumentsError::TooLarge);
    }
    let mut out = Out::default();
    out.len(entries.len());
    for entry in entries {
        // Both indices were checked against the scene, whose counts fit.
        if !fits_u32(entry.generator.len()) || !fits_u32(entry.document.len()) {
            return Err(EncodeDocumentsError::TooLarge);
        }
        out.len(entry.model);
        out.len(entry.part);
        out.bytes(entry.generator.as_bytes());
        out.u32(entry.version);
        out.bytes(entry.document);
    }
    Ok(out.0)
}

fn decode(payload: &[u8]) -> Result<Vec<Entry<'_>>, DecodeDocumentsError> {
    let mut input = In(payload);
    let malformed = DecodeDocumentsError::Malformed;
    let count = input.count(20).ok_or(malformed.clone())?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let model = input.len().ok_or(malformed.clone())?;
        let part = input.len().ok_or(malformed.clone())?;
        let generator = input
            .bytes()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .ok_or(malformed.clone())?;
        let version = input.u32().ok_or(malformed.clone())?;
        let document = input.bytes().ok_or(malformed.clone())?;
        entries.push(Entry {
            model,
            part,
            generator,
            version,
            document,
        });
    }
    if !input.is_empty() {
        return Err(malformed);
    }
    Ok(entries)
}

/// Every model and support leaf is a leaf, no two entries share a model, and
/// each support leaf belongs to one entry and is no model.
fn check_supports(supports: &[SupportEntry], scene: &Scene) -> Result<(), Broken> {
    let mut models = HashSet::new();
    for entry in supports {
        if !is_leaf(scene, entry.model) || !is_leaf(scene, entry.support) {
            return Err(Broken::NotALeaf);
        }
        if !models.insert(entry.model) {
            return Err(Broken::SameModel);
        }
    }
    let mut leaves = HashSet::new();
    for entry in supports {
        if models.contains(&entry.support) || !leaves.insert(entry.support) {
            return Err(Broken::SharedSupport);
        }
    }
    Ok(())
}

/// Every model is a leaf, every original a mesh, and no two entries share a
/// model.
fn check_hollows(hollows: &[HollowEntry], scene: &Scene) -> Result<(), Broken> {
    let mut models = HashSet::new();
    for entry in hollows {
        if !is_leaf(scene, entry.model) {
            return Err(Broken::NotALeaf);
        }
        if entry.original >= scene.meshes.len() {
            return Err(Broken::NoSuchMesh);
        }
        if !models.insert(entry.model) {
            return Err(Broken::SameModel);
        }
    }
    Ok(())
}

fn is_leaf(scene: &Scene, node: usize) -> bool {
    matches!(
        scene.nodes.get(node).map(|node| &node.kind),
        Some(NodeKind::Leaf { .. } | NodeKind::SharedLeaf { .. })
    )
}
