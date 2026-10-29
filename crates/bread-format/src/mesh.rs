/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The mesh chunk: one triangle mesh, lossless (`SPEC.md`, section 6).
//!
//! Vertices and triangles go through meshoptimizer's codecs, which keep every
//! vertex bit for bit. The triangle codec may start a triangle at another of
//! its corners (same winding), so the payload also lists those triangles and
//! the decoder turns them back: the mesh comes back identical, face order and
//! first corners included.
//!
//! Both codecs are pinned to version 1, whatever the library's default, so a
//! newer meshoptimizer cannot change what a chunk means. Facet normals a
//! source STL declared are not kept: they are file data, not geometry.
//!
//! ```text
//! metadata    u8 0 | 1, then name and comment (u8 0 | 1, u32 length, UTF-8)
//! vertices    u32 count, u32 length, meshopt vertex buffer of [f32; 3]
//! triangles   u32 count, u32 length, meshopt index buffer
//! rotations   u32 count, then per triangle: u32 gap from the previous
//!             listed triangle, u8 corner (1 | 2) the stored one starts at
//! ```

use std::fmt;

use crate::bytes::{In, Out};
use crate::container::ChunkTag;

/// The tag of a mesh chunk.
pub const MESH_TAG: ChunkTag = ChunkTag(*b"MESH");

/// The mesh payload layout this build writes and reads.
pub const MESH_VERSION: u16 = 1;

/// The first byte of a vertex buffer: attribute codec version 1.
const VERTEX_HEADER: u8 = 0xa1;
/// The first byte of an index buffer: triangle codec version 1.
const INDEX_HEADER: u8 = 0xe1;

/// The vertex codec version written, and its level: 2 is the library's
/// default, so the bytes match what the plain encoder writes.
const VERTEX_VERSION: i32 = 1;
const VERTEX_LEVEL: i32 = 2;

/// Bounds on what a damaged count may ask for, from the codecs' densest
/// output: each block of up to 256 vertices spends at least its 3-byte control
/// header, and each triangle at least a byte besides the index buffer's first
/// byte and 16-byte tail.
const VERTICES_PER_BLOCK: usize = 256;
const BLOCK_HEADER_BYTES: usize = 3;
const INDEX_BUFFER_OVERHEAD: usize = 17;

/// A triangle mesh, as a mesh chunk keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// The name and comment, or `None` for a mesh without metadata (which is
    /// not the same as metadata whose name and comment are both absent).
    pub meta: Option<MeshMeta>,
    /// The vertex positions: x, y and z.
    pub vertices: Vec<[f32; 3]>,
    /// The triangles: three indices into `vertices` each, first corner first.
    pub triangles: Vec<[u32; 3]>,
}

/// A mesh's name, and free text that came with it, such as an STL header.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeshMeta {
    pub name: Option<String>,
    pub comment: Option<String>,
}

/// Why a mesh could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeMeshError {
    /// A triangle names a vertex the mesh does not have.
    VertexOutOfRange,
    /// The mesh has 2^32 or more vertices or triangles, or encodes to 4 GiB
    /// or more.
    TooLarge,
}

impl fmt::Display for EncodeMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::VertexOutOfRange => "a triangle names a vertex the mesh does not have",
            Self::TooLarge => "the mesh is too large for a mesh chunk",
        })
    }
}

impl std::error::Error for EncodeMeshError {}

/// Why a mesh payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeMeshError {
    /// The payload is cut short, padded or self-contradictory.
    Malformed,
    /// A triangle names a vertex the mesh does not have.
    VertexOutOfRange,
}

impl fmt::Display for DecodeMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "the mesh data is malformed",
            Self::VertexOutOfRange => "a triangle names a vertex the mesh does not have",
        })
    }
}

impl std::error::Error for DecodeMeshError {}

/// Encodes `mesh` as a mesh chunk payload.
pub fn encode_mesh(mesh: &Mesh) -> Result<Vec<u8>, EncodeMeshError> {
    let vertex_count = mesh.vertices.len();
    if !fits_u32(vertex_count) || !fits_u32(mesh.triangles.len()) {
        return Err(EncodeMeshError::TooLarge);
    }
    let indices = mesh.triangles.as_flattened();
    if indices.iter().any(|&index| index as usize >= vertex_count) {
        return Err(EncodeMeshError::VertexOutOfRange);
    }
    let encoded_vertices =
        meshopt::encode_vertex_buffer_level(&mesh.vertices, VERTEX_LEVEL, Some(VERTEX_VERSION))
            .expect("12-byte vertices encode");
    let encoded_indices =
        meshopt::encode_index_buffer(indices, vertex_count).expect("whole triangles encode");
    // The triangle codec's version can only be set through a global, unsafe
    // switch, so its default is checked instead.
    assert_eq!(
        encoded_indices.first(),
        Some(&INDEX_HEADER),
        "the triangle codec writes version 1"
    );
    if !fits_u32(encoded_vertices.len()) || !fits_u32(encoded_indices.len()) {
        return Err(EncodeMeshError::TooLarge);
    }
    let stored: Vec<u32> = meshopt::decode_index_buffer(&encoded_indices, indices.len())
        .expect("the codec reads its own output");

    let mut out = Out::default();
    match &mesh.meta {
        None => out.u8(0),
        Some(meta) => {
            out.u8(1);
            out.text(meta.name.as_deref());
            out.text(meta.comment.as_deref());
        }
    }
    out.len(vertex_count);
    out.bytes(&encoded_vertices);
    out.len(mesh.triangles.len());
    out.bytes(&encoded_indices);

    let rotations: Vec<(usize, u8)> = stored
        .as_chunks::<3>()
        .0
        .iter()
        .zip(&mesh.triangles)
        .enumerate()
        .filter(|(_, (stored, original))| stored != original)
        .map(|(triangle, (stored, original))| {
            let corner = (1..3u8)
                .find(|&r| (0..3).all(|k| stored[(k + usize::from(r)) % 3] == original[k]))
                .expect("the codec only rotates triangles");
            (triangle, corner)
        })
        .collect();
    out.len(rotations.len());
    let mut previous = 0;
    for (triangle, corner) in rotations {
        out.len(triangle - previous);
        out.u8(corner);
        previous = triangle;
    }
    Ok(out.0)
}

fn fits_u32(n: usize) -> bool {
    u32::try_from(n).is_ok()
}

/// Decodes a mesh chunk payload.
pub fn decode_mesh(payload: &[u8]) -> Result<Mesh, DecodeMeshError> {
    decode(&mut In(payload)).ok_or(DecodeMeshError::Malformed)?
}

fn decode(input: &mut In<'_>) -> Option<Result<Mesh, DecodeMeshError>> {
    let meta = match input.u8()? {
        0 => None,
        1 => Some(MeshMeta {
            name: input.text()?,
            comment: input.text()?,
        }),
        _ => return None,
    };
    let vertex_count = input.len()?;
    let encoded_vertices = input.bytes()?;
    if encoded_vertices.first() != Some(&VERTEX_HEADER)
        || vertex_count.checked_mul(BLOCK_HEADER_BYTES)?
            >= encoded_vertices.len().checked_mul(VERTICES_PER_BLOCK)?
    {
        return None;
    }
    let triangle_count = input.len()?;
    let encoded_indices = input.bytes()?;
    if encoded_indices.first() != Some(&INDEX_HEADER)
        || triangle_count > encoded_indices.len().checked_sub(INDEX_BUFFER_OVERHEAD)?
    {
        return None;
    }
    let vertices: Vec<[f32; 3]> =
        meshopt::decode_vertex_buffer(encoded_vertices, vertex_count).ok()?;
    let mut indices: Vec<u32> =
        meshopt::decode_index_buffer(encoded_indices, triangle_count.checked_mul(3)?).ok()?;

    let faces = indices.as_chunks_mut::<3>().0;
    let rotations = input.count(5)?;
    let mut triangle = 0usize;
    for listed in 0..rotations {
        let gap = input.len()?;
        if listed > 0 && gap == 0 {
            return None;
        }
        triangle = triangle.checked_add(gap)?;
        let corner = usize::from(input.u8()?);
        if !(1..3).contains(&corner) {
            return None;
        }
        faces.get_mut(triangle)?.rotate_left(corner);
    }
    if !input.is_empty() {
        return None;
    }
    if indices.iter().any(|&index| index as usize >= vertex_count) {
        return Some(Err(DecodeMeshError::VertexOutOfRange));
    }
    Some(Ok(Mesh {
        meta,
        vertices,
        triangles: indices.as_chunks::<3>().0.to_vec(),
    }))
}
