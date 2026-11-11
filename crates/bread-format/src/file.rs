/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! A whole `.bread` file: the thumbnail when there is one, the scene chunk,
//! the project settings, the supports, the hollows and the surface paint when
//! there are any, then one mesh chunk per mesh of the scene, in its order, and
//! last the support previews when there are any.
//!
//! Reading skips the chunks this build does not know, official or private,
//! and writing leaves them out, as section 7.2 allows. It also skips supports
//! and hollows chunks of version 1, which only drafts of the format wrote, as
//! section 7.3 allows: their models open without editable supports, and their
//! hollows without the original. The support previews of draft supports are
//! skipped with them, since they could not be checked against the supports.

use std::fmt;
use std::io::{self, Read, Write};

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, HOLLOWS_VERSION, HollowEntry,
    SETTINGS_TAG, SETTINGS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, SupportEntry, decode_hollows,
    decode_supports, encode_hollows, encode_supports,
};
use crate::container::{BreadReader, BreadWriter, ChunkTag, ReadBreadError};
use crate::mesh::{DecodeMeshError, EncodeMeshError, MESH_TAG, MESH_VERSION, Mesh, decode_mesh};
use crate::mesh_v2::{decode_mesh_v2, encode_mesh_v2};
use crate::parallel;
use crate::preview::{
    DecodePreviewsError, EncodePreviewsError, SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION,
    SupportPreview, decode_previews, encode_previews,
};
use crate::scene::{
    DecodeSceneError, EncodeSceneError, SCENE_TAG, SCENE_VERSION, Scene, decode_scene, encode_scene,
};
use crate::surface::{
    DecodeSurfaceError, EncodeSurfaceError, SURFACE_TAG, SURFACE_VERSION, SurfaceEntry,
    decode_surface, encode_surface,
};

/// The tag of the thumbnail chunk, whose payload is a PNG file. Section 8
/// asks writers to put it first, so a program that only wants the picture
/// reads one chunk.
pub const THUMBNAIL_TAG: ChunkTag = ChunkTag(*b"THMB");

/// The thumbnail chunk version this build writes and reads.
pub const THUMBNAIL_VERSION: u16 = 1;

/// What a `.bread` file holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BreadFile {
    /// The scene and its meshes.
    pub scene: Scene,
    /// The project settings document, carried without being read.
    pub settings: Option<Vec<u8>>,
    /// Supports kept as data. Their support leaves show an empty mesh.
    pub supports: Vec<SupportEntry>,
    /// How hollowed models were made. Their leaves show the hollow.
    pub hollows: Vec<HollowEntry>,
    /// A small picture of the file: a PNG, at most 512 x 512 and best
    /// 256 x 256, in sRGB. It is carried without being read: the PNG is
    /// neither checked nor decoded.
    pub thumbnail: Option<Vec<u8>>,
    /// The mesh of the supports, for readers that do not rebuild them from
    /// their documents. Each preview names the support leaf of an entry of
    /// `supports`. A program that edits the supports writes new previews, or
    /// none.
    pub support_previews: Vec<SupportPreview>,
    /// What was painted on the faces of leaves: one mask per leaf and kind,
    /// each document carried without being read.
    pub surface: Vec<SurfaceEntry>,
}

/// Why a file could not be written.
#[derive(Debug)]
pub enum WriteFileError {
    Io(io::Error),
    Scene(EncodeSceneError),
    Mesh(EncodeMeshError),
    Supports(EncodeDocumentsError),
    Hollows(EncodeDocumentsError),
    Previews(EncodePreviewsError),
    Surface(EncodeSurfaceError),
    /// A support leaf shows a mesh with vertices or triangles: support
    /// leaves are stored with an empty mesh.
    SupportMeshNotEmpty {
        leaf: usize,
    },
    /// A support leaf is also a hollowed model.
    SupportIsHollowed {
        leaf: usize,
    },
    /// A support preview names a leaf that no supports entry shows.
    PreviewWithoutSupports {
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
            Self::Previews(err) => write!(f, "{err}"),
            Self::Surface(err) => write!(f, "{err}"),
            Self::SupportMeshNotEmpty { leaf } => {
                write!(
                    f,
                    "the support leaf {leaf} shows a mesh instead of an empty one"
                )
            }
            Self::SupportIsHollowed { leaf } => write!(f, "{}", SupportIsHollowed(*leaf)),
            Self::PreviewWithoutSupports { leaf } => {
                write!(f, "{}", PreviewWithoutSupports(*leaf))
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
    Previews(DecodePreviewsError),
    Surface(DecodeSurfaceError),
    /// A support leaf is also a hollowed model.
    SupportIsHollowed {
        leaf: usize,
    },
    /// A support preview names a leaf that no supports entry shows.
    PreviewWithoutSupports {
        leaf: usize,
    },
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
            Self::Previews(err) => write!(f, "{err}"),
            Self::Surface(err) => write!(f, "{err}"),
            Self::SupportIsHollowed { leaf } => write!(f, "{}", SupportIsHollowed(*leaf)),
            Self::PreviewWithoutSupports { leaf } => {
                write!(f, "{}", PreviewWithoutSupports(*leaf))
            }
        }
    }
}

impl std::error::Error for ReadFileError {}

/// The message of both `SupportIsHollowed` errors.
struct SupportIsHollowed(usize);

impl fmt::Display for SupportIsHollowed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the support leaf {} is also a hollowed model", self.0)
    }
}

/// The message of both `PreviewWithoutSupports` errors.
struct PreviewWithoutSupports(usize);

impl fmt::Display for PreviewWithoutSupports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the support preview of leaf {} has no supports entry",
            self.0
        )
    }
}

impl From<ReadBreadError> for ReadFileError {
    fn from(err: ReadBreadError) -> Self {
        Self::File(err)
    }
}

/// The newest version of the chunk `tag` this build reads, or `None` for a
/// tag it does not know, which a reader skips. Every version from 1 up to it
/// is read, though older supports and hollows chunks are skipped; any other
/// version makes the file unreadable.
pub fn supported_version(tag: ChunkTag) -> Option<u16> {
    Some(match tag {
        THUMBNAIL_TAG => THUMBNAIL_VERSION,
        SCENE_TAG => SCENE_VERSION,
        SETTINGS_TAG => SETTINGS_VERSION,
        SUPPORTS_TAG => SUPPORTS_VERSION,
        HOLLOWS_TAG => HOLLOWS_VERSION,
        MESH_TAG => MESH_VERSION,
        SUPPORT_PREVIEW_TAG => SUPPORT_PREVIEW_VERSION,
        SURFACE_TAG => SURFACE_VERSION,
        _ => return None,
    })
}

/// Writes `file` as a `.bread` file to `out`, handing `out` back. When it
/// fails, what it wrote to `out` is not a valid file.
pub fn write_file<W: Write>(file: &BreadFile, out: W) -> Result<W, WriteFileError> {
    let scene = &file.scene;
    let scene_payload = encode_scene(scene).map_err(WriteFileError::Scene)?;
    let supports = encode_supports(&file.supports, scene).map_err(WriteFileError::Supports)?;
    let hollows = encode_hollows(&file.hollows, scene).map_err(WriteFileError::Hollows)?;
    let previews =
        encode_previews(&file.support_previews, scene).map_err(WriteFileError::Previews)?;
    let surface = encode_surface(&file.surface, scene).map_err(WriteFileError::Surface)?;
    for entry in &file.supports {
        let leaf = entry.support;
        if scene
            .leaf_mesh(leaf)
            .is_some_and(|mesh| !mesh.vertices.is_empty() || !mesh.triangles.is_empty())
        {
            return Err(WriteFileError::SupportMeshNotEmpty { leaf });
        }
    }
    if let Some(leaf) = hollowed_support(&file.supports, &file.hollows) {
        return Err(WriteFileError::SupportIsHollowed { leaf });
    }
    if let Some(leaf) = unsupported_preview(&file.support_previews, &file.supports) {
        return Err(WriteFileError::PreviewWithoutSupports { leaf });
    }
    // Each mesh is encoded on its own, so they are encoded side by side.
    let meshes: Vec<Vec<u8>> = parallel::map(&scene.meshes, encode_mesh_v2)
        .into_iter()
        .collect::<Result<_, _>>()
        .map_err(WriteFileError::Mesh)?;

    let mut writer = BreadWriter::new(out)?;
    if let Some(thumbnail) = &file.thumbnail {
        writer.chunk(THUMBNAIL_TAG, THUMBNAIL_VERSION, thumbnail)?;
    }
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
    if !file.surface.is_empty() {
        writer.chunk(SURFACE_TAG, SURFACE_VERSION, &surface)?;
    }
    for payload in &meshes {
        writer.chunk(MESH_TAG, MESH_VERSION, payload)?;
    }
    if !file.support_previews.is_empty() {
        writer.chunk(SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION, &previews)?;
    }
    Ok(writer.finish()?)
}

/// Mesh payloads smaller than this are decoded where they are read: a thread
/// of their own would cost more than it saves.
const MESH_ON_ITS_OWN_THREAD: usize = 1 << 16;

/// A mesh chunk being decoded, or decoded already.
enum PendingMesh<'scope> {
    Done(Result<Mesh, DecodeMeshError>),
    Running(std::thread::ScopedJoinHandle<'scope, Result<Mesh, DecodeMeshError>>),
}

impl PendingMesh<'_> {
    fn finish(self) -> Result<Mesh, DecodeMeshError> {
        match self {
            Self::Done(mesh) => mesh,
            Self::Running(handle) => handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        }
    }
}

fn decode_mesh_chunk(version: u16, payload: &[u8]) -> Result<Mesh, DecodeMeshError> {
    match version {
        1 => decode_mesh(payload),
        _ => decode_mesh_v2(payload),
    }
}

/// Reads the `.bread` file in `input`.
///
/// Large meshes are decoded on threads of their own while the rest of the
/// file is read. A file is refused for the same reason as if every chunk
/// were read in order: the first fault in file order.
pub fn read_file<R: Read>(input: R) -> Result<BreadFile, ReadFileError> {
    let mut reader = BreadReader::new(input)?;
    std::thread::scope(|scope| read_chunks(&mut reader, scope))
}

fn read_chunks<'scope, R: Read>(
    reader: &mut BreadReader<R>,
    scope: &'scope std::thread::Scope<'scope, '_>,
) -> Result<BreadFile, ReadFileError> {
    let (mut thumbnails, mut scenes, mut settings, mut supports, mut hollows, mut previews) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut surfaces = Vec::new();
    let mut pending: Vec<PendingMesh<'scope>> = Vec::new();
    let mut running = std::collections::VecDeque::new();
    let threads = parallel::threads();
    let mut read_all = || -> Result<(), ReadFileError> {
        while let Some(chunk) = reader.next_chunk()? {
            let Some(supported) = supported_version(chunk.tag) else {
                continue;
            };
            if chunk.version == 0 || chunk.version > supported {
                return Err(ReadFileError::ChunkVersion {
                    tag: chunk.tag,
                    version: chunk.version,
                });
            }
            // An older supports or hollows chunk is a draft one: it still counts
            // as the one chunk of its tag, but its payload is not read.
            let current = chunk.version == supported;
            match chunk.tag {
                THUMBNAIL_TAG => thumbnails.push(chunk.payload),
                SCENE_TAG => scenes.push(chunk.payload),
                SETTINGS_TAG => settings.push(chunk.payload),
                SUPPORTS_TAG => supports.push(current.then_some(chunk.payload)),
                HOLLOWS_TAG => hollows.push(current.then_some(chunk.payload)),
                SUPPORT_PREVIEW_TAG => previews.push(chunk.payload),
                SURFACE_TAG => surfaces.push(chunk.payload),
                _ => {
                    let (version, payload) = (chunk.version, chunk.payload);
                    if threads < 2 || payload.len() < MESH_ON_ITS_OWN_THREAD {
                        pending.push(PendingMesh::Done(decode_mesh_chunk(version, &payload)));
                        continue;
                    }
                    // At most as many meshes at once as the machine runs threads.
                    if running.len() >= threads {
                        let oldest: usize = running.pop_front().expect("one is running");
                        let mesh = std::mem::replace(
                            &mut pending[oldest],
                            PendingMesh::Done(Err(DecodeMeshError::Malformed)),
                        );
                        pending[oldest] = PendingMesh::Done(mesh.finish());
                    }
                    running.push_back(pending.len());
                    pending.push(PendingMesh::Running(
                        scope.spawn(move || decode_mesh_chunk(version, &payload)),
                    ));
                }
            }
        }
        Ok(())
    };
    let read = read_all();
    // A mesh refused comes before any fault found after it in the file, as
    // when each mesh is decoded the moment it is read.
    let meshes = pending
        .into_iter()
        .map(PendingMesh::finish)
        .collect::<Result<Vec<_>, _>>()
        .map_err(ReadFileError::Mesh)?;
    read?;
    let scene = single(SCENE_TAG, scenes)?.ok_or(ReadFileError::ChunkCount {
        tag: SCENE_TAG,
        count: 0,
    })?;
    let thumbnail = single(THUMBNAIL_TAG, thumbnails)?;
    let settings = single(SETTINGS_TAG, settings)?;
    let supports = single(SUPPORTS_TAG, supports)?;
    let draft_supports = matches!(supports, Some(None));
    let supports = supports.flatten();
    let hollows = single(HOLLOWS_TAG, hollows)?.flatten();
    let previews = single(SUPPORT_PREVIEW_TAG, previews)?;
    let surface = single(SURFACE_TAG, surfaces)?;

    let scene = decode_scene(&scene, meshes).map_err(ReadFileError::Scene)?;
    let supports = match supports {
        None => Vec::new(),
        Some(payload) => decode_supports(&payload, &scene).map_err(ReadFileError::Supports)?,
    };
    let hollows = match hollows {
        None => Vec::new(),
        Some(payload) => decode_hollows(&payload, &scene).map_err(ReadFileError::Hollows)?,
    };
    if let Some(leaf) = hollowed_support(&supports, &hollows) {
        return Err(ReadFileError::SupportIsHollowed { leaf });
    }
    let support_previews = match previews {
        Some(payload) if !draft_supports => {
            decode_previews(&payload, &scene).map_err(ReadFileError::Previews)?
        }
        _ => Vec::new(),
    };
    if let Some(leaf) = unsupported_preview(&support_previews, &supports) {
        return Err(ReadFileError::PreviewWithoutSupports { leaf });
    }
    let surface = match surface {
        None => Vec::new(),
        Some(payload) => decode_surface(&payload, &scene).map_err(ReadFileError::Surface)?,
    };
    Ok(BreadFile {
        scene,
        settings,
        supports,
        hollows,
        thumbnail,
        support_previews,
        surface,
    })
}

/// A support leaf that is also the model of a hollow, which section 7.3
/// forbids across the two chunks.
fn hollowed_support(supports: &[SupportEntry], hollows: &[HollowEntry]) -> Option<usize> {
    supports
        .iter()
        .map(|entry| entry.support)
        .find(|&leaf| hollows.iter().any(|hollow| hollow.model == leaf))
}

/// A preview's leaf that is not the support leaf of any supports entry,
/// which section 9 forbids.
fn unsupported_preview(previews: &[SupportPreview], supports: &[SupportEntry]) -> Option<usize> {
    previews
        .iter()
        .map(|preview| preview.leaf)
        .find(|&leaf| !supports.iter().any(|entry| entry.support == leaf))
}

/// The chunk of a tag that comes at most once.
fn single<T>(tag: ChunkTag, chunks: Vec<T>) -> Result<Option<T>, ReadFileError> {
    if chunks.len() > 1 {
        return Err(ReadFileError::ChunkCount {
            tag,
            count: chunks.len(),
        });
    }
    Ok(chunks.into_iter().next())
}
