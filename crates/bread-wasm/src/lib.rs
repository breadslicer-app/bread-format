/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The `bread-format` crate for JavaScript: built to WebAssembly, so that the
//! browser reads `.bread` files with the same code, checks and all, as every
//! other program.
//!
//! The functions here are plain Rust and tested natively: [`read_chunks`]
//! lists a file's chunks, [`decode_mesh_chunk`] decodes a mesh chunk, and
//! [`decode_scene_chunk`] the scene chunk. [`abi`] puts them behind a small C
//! ABI that the JavaScript package calls.
//!
//! With the `write` feature, `read_stl_mesh` reads an STL and `write_bread`
//! writes a file, behind [`write_abi`]. Writing takes zstd's compressor and
//! meshoptimizer's encoders, which more than double the module, so the
//! JavaScript package builds it twice: `bread.wasm` reads, and
//! `bread-write.wasm` also writes.

#[cfg(feature = "write")]
use bread_format::{
    BreadFile, EncodeMeshError, EncodeSceneError, ReadStlError, WriteFileError, read_stl,
    write_file,
};
use bread_format::{
    BreadReader, Chunk, ChunkTag, DecodeMeshError, MESH_TAG, Mesh, ReadBreadError, SCENE_TAG,
    Scene, decode_mesh, decode_mesh_v2, decode_scene, supported_version,
};

pub mod abi;
#[cfg(all(feature = "write", target_family = "wasm"))]
mod cxx;
#[cfg(feature = "write")]
pub mod write_abi;

/// Why a file was refused: a reason code of `conformance/README.md`, and the
/// chunk it is about, when it is about one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: &'static str,
    pub chunk: Option<ChunkTag>,
}

/// The chunks of the file `bytes` that this build knows, in file order, each
/// checked and decompressed. Unknown chunks, official or private, are
/// skipped; a known chunk at a version this build does not read refuses the
/// file. Older supports and hollows chunks are kept, at their version, for
/// the reader to skip.
pub fn read_chunks(bytes: &[u8]) -> Result<Vec<Chunk>, Refusal> {
    let mut reader = BreadReader::new(bytes).map_err(refusal)?;
    let mut chunks = Vec::new();
    while let Some(chunk) = reader.next_chunk().map_err(refusal)? {
        let Some(supported) = supported_version(chunk.tag) else {
            continue;
        };
        if chunk.version == 0 || chunk.version > supported {
            return Err(Refusal {
                reason: "version",
                chunk: Some(chunk.tag),
            });
        }
        chunks.push(chunk);
    }
    Ok(chunks)
}

/// The mesh in the payload of a mesh chunk of `version`, as [`read_chunks`]
/// gives it.
pub fn decode_mesh_chunk(version: u16, payload: &[u8]) -> Result<Mesh, Refusal> {
    let refused = |reason| Refusal {
        reason,
        chunk: Some(MESH_TAG),
    };
    let decoded = match version {
        1 => decode_mesh(payload),
        2 => decode_mesh_v2(payload),
        _ => return Err(refused("version")),
    };
    decoded.map_err(|err| {
        refused(match err {
            DecodeMeshError::Malformed => "malformed",
            DecodeMeshError::VertexOutOfRange => "no-such-vertex",
        })
    })
}

/// The scene in the payload of a scene chunk of `version`, as [`read_chunks`]
/// gives it, in a file of `mesh_count` mesh chunks. The scene names its
/// meshes by index and holds none of them: its `meshes` are `mesh_count`
/// empty ones, and [`decode_mesh_chunk`] decodes each.
pub fn decode_scene_chunk(
    version: u16,
    payload: &[u8],
    mesh_count: usize,
) -> Result<Scene, Refusal> {
    let refused = |reason| Refusal {
        reason,
        chunk: Some(SCENE_TAG),
    };
    if version != 1 {
        return Err(refused("version"));
    }
    // `mesh_count` is how many mesh chunks the file really holds, not what
    // the payload claims, so these placeholders cost what the file does. A
    // scene that names another number of meshes is malformed (SPEC.md, 5.1).
    decode_scene(payload, vec![Mesh::default(); mesh_count]).map_err(|_| refused("malformed"))
}

#[cfg(feature = "write")]
/// The mesh of an STL file, binary or ASCII, as [`read_stl`] reads it.
/// Refused as `not-stl`, `bad-stl` (an ASCII STL that breaks its grammar) or
/// `too-large`, about no chunk.
pub fn read_stl_mesh(bytes: &[u8]) -> Result<Mesh, Refusal> {
    read_stl(bytes).map_err(|err| Refusal {
        reason: match err {
            ReadStlError::NotStl => "not-stl",
            ReadStlError::Ascii { .. } => "bad-stl",
            ReadStlError::TooLarge => "too-large",
        },
        chunk: None,
    })
}

#[cfg(feature = "write")]
/// A `.bread` file of `meshes` and the scene that places them, given as a
/// scene chunk payload over them (`SPEC.md`, section 5). The payload is
/// decoded first, with every check a reader makes, so a scene a reader would
/// refuse is refused here as `malformed`, and never written. A mesh is
/// refused as `no-such-vertex` when a triangle names a vertex it does not
/// have, and as `too-large` past what a mesh chunk holds.
pub fn write_bread(meshes: Vec<Mesh>, scene_payload: &[u8]) -> Result<Vec<u8>, Refusal> {
    let scene = decode_scene(scene_payload, meshes).map_err(|_| Refusal {
        reason: "malformed",
        chunk: Some(SCENE_TAG),
    })?;
    let file = BreadFile {
        scene,
        ..BreadFile::default()
    };
    write_file(&file, Vec::new()).map_err(|err| {
        let (reason, chunk) = match err {
            WriteFileError::Mesh(EncodeMeshError::VertexOutOfRange) => ("no-such-vertex", MESH_TAG),
            WriteFileError::Mesh(_) => ("too-large", MESH_TAG),
            WriteFileError::Scene(EncodeSceneError::TooLarge) => ("too-large", SCENE_TAG),
            // The scene was decoded with every check above, and the file
            // holds nothing else.
            _ => ("malformed", SCENE_TAG),
        };
        Refusal {
            reason,
            chunk: Some(chunk),
        }
    })
}

fn refusal(err: ReadBreadError) -> Refusal {
    let (reason, chunk) = match err {
        ReadBreadError::NotBread => ("not-bread", None),
        ReadBreadError::UnsupportedVersion(_) => ("version", None),
        // Reading from memory only fails by running out of bytes.
        ReadBreadError::Truncated | ReadBreadError::Io(_) => ("cut-short", None),
        ReadBreadError::Corrupt(tag) => ("crc", Some(tag)),
        ReadBreadError::BadPayload(tag) => ("bad-payload", Some(tag)),
    };
    Refusal { reason, chunk }
}
