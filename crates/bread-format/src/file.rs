/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! A whole `.bread` file: the scene chunk, the project settings, the supports
//! and the hollows when there are any, then one mesh chunk per mesh of the
//! scene, in its order.
//!
//! Reading skips the chunks this build does not know, official or private,
//! and writing leaves them out, as section 7.2 allows.

use std::fmt;
use std::io::{self, Read, Write};

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, HOLLOWS_VERSION, LeafDocument,
    SETTINGS_TAG, SETTINGS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, decode_documents,
    encode_documents,
};
use crate::container::{BreadReader, BreadWriter, ChunkTag, ReadBreadError};
use crate::mesh::{
    DecodeMeshError, EncodeMeshError, MESH_TAG, MESH_VERSION, decode_mesh, encode_mesh,
};
use crate::scene::{
    DecodeSceneError, EncodeSceneError, SCENE_TAG, SCENE_VERSION, Scene, decode_scene, encode_scene,
};

/// What a `.bread` file holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BreadFile {
    /// The scene and its meshes.
    pub scene: Scene,
    /// The project settings document, carried without being read.
    pub settings: Option<Vec<u8>>,
    /// Supports kept as data. Their leaves show an empty mesh.
    pub supports: Vec<LeafDocument>,
    /// How hollowed models were made. Their leaves keep the hollow.
    pub hollows: Vec<LeafDocument>,
}

/// Why a file could not be written.
#[derive(Debug)]
pub enum WriteFileError {
    Io(io::Error),
    Scene(EncodeSceneError),
    Mesh(EncodeMeshError),
    Supports(EncodeDocumentsError),
    Hollows(EncodeDocumentsError),
    /// A support leaf shows a mesh with vertices or triangles: support
    /// leaves are stored with an empty mesh.
    SupportMeshNotEmpty {
        leaf: usize,
    },
}

impl fmt::Display for WriteFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Scene(err) => write!(f, "{err}"),
            Self::Mesh(err) => write!(f, "{err}"),
            Self::Supports(err) | Self::Hollows(err) => write!(f, "{err}"),
            Self::SupportMeshNotEmpty { leaf } => {
                write!(
                    f,
                    "the support leaf {leaf} shows a mesh instead of an empty one"
                )
            }
        }
    }
}

impl std::error::Error for WriteFileError {}

impl From<io::Error> for WriteFileError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Why a `.bread` file could not be read.
#[derive(Debug)]
pub enum ReadFileError {
    File(ReadBreadError),
    /// A chunk this build reads has a version it does not read: a newer one,
    /// or 0, which no version is.
    ChunkVersion {
        tag: ChunkTag,
        version: u16,
    },
    /// The file holds a number of chunks of one tag that the format does not
    /// allow: no scene chunk, or two of a chunk that comes at most once.
    ChunkCount {
        tag: ChunkTag,
        count: usize,
    },
    Mesh(DecodeMeshError),
    Scene(DecodeSceneError),
    Supports(DecodeDocumentsError),
    Hollows(DecodeDocumentsError),
}

impl fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(err) => write!(f, "{err}"),
            Self::ChunkVersion { tag, version: 0 } => {
                write!(f, "the {tag} chunk has version 0, which does not exist")
            }
            Self::ChunkVersion { tag, version } => {
                write!(
                    f,
                    "the {tag} chunk is written in a newer version ({version})"
                )
            }
            Self::ChunkCount { tag, count } => write!(f, "the file holds {count} {tag} chunks"),
            Self::Mesh(err) => write!(f, "{err}"),
            Self::Scene(err) => write!(f, "{err}"),
            Self::Supports(err) | Self::Hollows(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ReadFileError {}

impl From<ReadBreadError> for ReadFileError {
    fn from(err: ReadBreadError) -> Self {
        Self::File(err)
    }
}

/// Writes `file` as a `.bread` file to `out`, handing `out` back. When it
/// fails, what it wrote to `out` is not a valid file.
pub fn write_file<W: Write>(file: &BreadFile, out: W) -> Result<W, WriteFileError> {
    let scene = &file.scene;
    for support in &file.supports {
        if scene
            .leaf_mesh(support.leaf)
            .is_some_and(|mesh| !mesh.vertices.is_empty() || !mesh.triangles.is_empty())
        {
            return Err(WriteFileError::SupportMeshNotEmpty { leaf: support.leaf });
        }
    }
    let scene_payload = encode_scene(scene).map_err(WriteFileError::Scene)?;
    let supports = encode_documents(&file.supports, scene).map_err(WriteFileError::Supports)?;
    let hollows = encode_documents(&file.hollows, scene).map_err(WriteFileError::Hollows)?;

    let mut writer = BreadWriter::new(out)?;
    writer.chunk(SCENE_TAG, SCENE_VERSION, &scene_payload)?;
    if let Some(settings) = &file.settings {
        writer.chunk(SETTINGS_TAG, SETTINGS_VERSION, settings)?;
    }
    if !file.supports.is_empty() {
        writer.chunk(SUPPORTS_TAG, SUPPORTS_VERSION, &supports)?;
    }
    if !file.hollows.is_empty() {
        writer.chunk(HOLLOWS_TAG, HOLLOWS_VERSION, &hollows)?;
    }
    for mesh in &scene.meshes {
        let payload = encode_mesh(mesh).map_err(WriteFileError::Mesh)?;
        writer.chunk(MESH_TAG, MESH_VERSION, &payload)?;
    }
    Ok(writer.finish()?)
}

/// Reads the `.bread` file in `input`.
pub fn read_file<R: Read>(input: R) -> Result<BreadFile, ReadFileError> {
    let mut reader = BreadReader::new(input)?;
    let (mut scenes, mut settings, mut supports, mut hollows) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut meshes = Vec::new();
    while let Some(chunk) = reader.next_chunk()? {
        let supported = match chunk.tag {
            SCENE_TAG => SCENE_VERSION,
            SETTINGS_TAG => SETTINGS_VERSION,
            SUPPORTS_TAG => SUPPORTS_VERSION,
            HOLLOWS_TAG => HOLLOWS_VERSION,
            MESH_TAG => MESH_VERSION,
            _ => continue,
        };
        if chunk.version == 0 || chunk.version > supported {
            return Err(ReadFileError::ChunkVersion {
                tag: chunk.tag,
                version: chunk.version,
            });
        }
        match chunk.tag {
            SCENE_TAG => scenes.push(chunk.payload),
            SETTINGS_TAG => settings.push(chunk.payload),
            SUPPORTS_TAG => supports.push(chunk.payload),
            HOLLOWS_TAG => hollows.push(chunk.payload),
            _ => meshes.push(decode_mesh(&chunk.payload).map_err(ReadFileError::Mesh)?),
        }
    }
    let scene = single(SCENE_TAG, scenes)?.ok_or(ReadFileError::ChunkCount {
        tag: SCENE_TAG,
        count: 0,
    })?;
    let settings = single(SETTINGS_TAG, settings)?;
    let supports = single(SUPPORTS_TAG, supports)?;
    let hollows = single(HOLLOWS_TAG, hollows)?;

    let scene = decode_scene(&scene, meshes).map_err(ReadFileError::Scene)?;
    let supports = match supports {
        None => Vec::new(),
        Some(payload) => decode_documents(&payload, &scene).map_err(ReadFileError::Supports)?,
    };
    let hollows = match hollows {
        None => Vec::new(),
        Some(payload) => decode_documents(&payload, &scene).map_err(ReadFileError::Hollows)?,
    };
    Ok(BreadFile {
        scene,
        settings,
        supports,
        hollows,
    })
}

/// The payload of a chunk that comes at most once.
fn single(tag: ChunkTag, chunks: Vec<Vec<u8>>) -> Result<Option<Vec<u8>>, ReadFileError> {
    if chunks.len() > 1 {
        return Err(ReadFileError::ChunkCount {
            tag,
            count: chunks.len(),
        });
    }
    Ok(chunks.into_iter().next())
}
