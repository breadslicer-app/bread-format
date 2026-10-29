/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The application chunks (`SPEC.md`, section 7.3): documents that belong to
//! one program, carried without being read.
//!
//! The project settings chunk is the settings document itself. The supports
//! and hollows chunks share one envelope: a list of documents, each keyed by
//! the scene leaf it belongs to and named by the generator that wrote it.
//! The crate checks the envelope (each entry names a leaf of the scene) and
//! never reads the documents: telling which generators and document versions
//! it understands is up to the program.
//!
//! ```text
//! entries   u32 count, then per entry:
//!           u32 the leaf's node index in the scene chunk
//!           u32 length, UTF-8 generator name
//!           u32 generator document version
//!           u32 length, document bytes
//! ```

use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::scene::{NodeKind, Scene};

/// The tag of the project settings chunk, whose payload is the settings
/// document: UTF-8 TOML that starts with its own version.
pub const SETTINGS_TAG: ChunkTag = ChunkTag(*b"PROJ");

/// The project settings chunk version this build writes and reads.
pub const SETTINGS_VERSION: u16 = 1;

/// The tag of the supports chunk. Its leaves are stored with an empty mesh,
/// which the generator rebuilds from the document.
pub const SUPPORTS_TAG: ChunkTag = ChunkTag(*b"SUPP");

/// The supports payload layout this build writes and reads.
pub const SUPPORTS_VERSION: u16 = 1;

/// The tag of the hollows chunk. Its leaves keep their mesh, the hollow.
pub const HOLLOWS_TAG: ChunkTag = ChunkTag(*b"HOLW");

/// The hollows payload layout this build writes and reads.
pub const HOLLOWS_VERSION: u16 = 1;

/// A program's document about one leaf of the scene, such as its supports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafDocument {
    /// The leaf, as an index into the scene's nodes.
    pub leaf: usize,
    /// Which generator wrote `document`, such as `"df"`.
    pub generator: String,
    /// The generator's own version of `document`.
    pub version: u32,
    pub document: Vec<u8>,
}

/// Why a list of leaf documents could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeDocumentsError {
    /// A document names a node the scene does not have, or one that is not
    /// a leaf.
    NotALeaf,
    /// There are 2^32 or more documents, or a generator name or a document
    /// of 4 GiB or more.
    TooLarge,
}

impl fmt::Display for EncodeDocumentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotALeaf => "a document belongs to a scene part that is not a model",
            Self::TooLarge => "the documents are too large for their chunk",
        })
    }
}

impl std::error::Error for EncodeDocumentsError {}

/// Why a supports or hollows payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeDocumentsError {
    /// The payload is cut short, padded, or not text where text belongs.
    Malformed,
    /// An entry names a node the scene does not have, or one that is not a
    /// leaf.
    NotALeaf,
}

impl fmt::Display for DecodeDocumentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "the document data is malformed",
            Self::NotALeaf => "a document belongs to a scene part that is not a model",
        })
    }
}

impl std::error::Error for DecodeDocumentsError {}

/// Encodes `documents` as a supports or hollows payload, checking that each
/// names a leaf of `scene`.
pub fn encode_documents(
    documents: &[LeafDocument],
    scene: &Scene,
) -> Result<Vec<u8>, EncodeDocumentsError> {
    if !fits_u32(documents.len()) {
        return Err(EncodeDocumentsError::TooLarge);
    }
    let mut out = Out::default();
    out.len(documents.len());
    for document in documents {
        if !is_leaf(scene, document.leaf) {
            return Err(EncodeDocumentsError::NotALeaf);
        }
        if !fits_u32(document.generator.len()) || !fits_u32(document.document.len()) {
            return Err(EncodeDocumentsError::TooLarge);
        }
        out.len(document.leaf);
        out.bytes(document.generator.as_bytes());
        out.u32(document.version);
        out.bytes(&document.document);
    }
    Ok(out.0)
}

/// Decodes a supports or hollows payload, checking that each entry names a
/// leaf of `scene`, the scene of the same file.
pub fn decode_documents(
    payload: &[u8],
    scene: &Scene,
) -> Result<Vec<LeafDocument>, DecodeDocumentsError> {
    let mut input = In(payload);
    let malformed = DecodeDocumentsError::Malformed;
    let count = input.count(16).ok_or(malformed.clone())?;
    let mut documents = Vec::with_capacity(count);
    for _ in 0..count {
        let leaf = input.len().ok_or(malformed.clone())?;
        let generator = input
            .bytes()
            .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
            .ok_or(malformed.clone())?;
        let version = input.u32().ok_or(malformed.clone())?;
        let document = input.bytes().ok_or(malformed.clone())?.to_vec();
        if !is_leaf(scene, leaf) {
            return Err(DecodeDocumentsError::NotALeaf);
        }
        documents.push(LeafDocument {
            leaf,
            generator,
            version,
            document,
        });
    }
    if !input.is_empty() {
        return Err(malformed);
    }
    Ok(documents)
}

fn is_leaf(scene: &Scene, node: usize) -> bool {
    matches!(
        scene.nodes.get(node).map(|node| &node.kind),
        Some(NodeKind::Leaf { .. } | NodeKind::SharedLeaf { .. })
    )
}
