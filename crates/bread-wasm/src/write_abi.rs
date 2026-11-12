/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The C ABI for writing, built with the `write` feature: reading an STL into
//! the mesh the mesh accessors of [`crate::abi`] give, and writing a file
//! from meshes JavaScript adds one by one and a scene that places them.
//! Writing takes zstd's compressor and meshoptimizer's encoders, which more
//! than double the module, so a page that only reads loads the module built
//! without it.

#![allow(unsafe_code)]

use std::cell::RefCell;

use bread_format::{MESH_TAG, Mesh, Meta};

use crate::Refusal;
use crate::abi::{MESH, REFUSAL};
use crate::{read_stl_mesh, write_bread};

thread_local! {
    /// The meshes of the file being written, in order.
    static WRITING: RefCell<Vec<Mesh>> = const { RefCell::new(Vec::new()) };
    /// The last file written.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Drops the file being written and the last one written.
pub(crate) fn release() {
    WRITING.set(Vec::new());
    OUTPUT.set(Vec::new());
}

/// Reads the STL file in the `len` bytes at `ptr`, as [`read_stl_mesh`]
/// does, into the mesh the mesh accessors give. Returns 1 when it was read
/// and 0 when it was refused.
///
/// # Safety
///
/// As for [`bread_read_chunks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_read_stl(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let (mesh, refusal) = match read_stl_mesh(bytes) {
        Ok(mesh) => (Some(mesh), None),
        Err(refusal) => (None, Some(refusal)),
    };
    MESH.set(mesh);
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// Starts a file to write: drops the meshes added so far.
#[unsafe(no_mangle)]
pub extern "C" fn bread_write_begin() {
    WRITING.set(Vec::new());
}

/// The text of `len` bytes at `ptr`, or `None` for a null `ptr`; `Err` when
/// it is not UTF-8.
///
/// # Safety
///
/// A non-null `ptr` points to `len` initialized bytes.
unsafe fn text_at(ptr: *const u8, len: usize) -> Result<Option<String>, ()> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller promises `len` readable bytes at a non-null `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8(bytes.to_vec()).map(Some).map_err(|_| ())
}

/// Adds the next mesh of the file being written. Its vertices are the
/// `vertices_len` bytes at `vertices`, x, y and z of each as little-endian
/// `f32`; its triangles the `triangles_len` bytes at `triangles`, three
/// vertex indices each as little-endian `u32`. With `has_meta` 1 it has
/// metadata: a name and a comment, each UTF-8 at its pointer, or none for a
/// null pointer. Returns 1 when the mesh was added, and 0 when it was refused
/// as `malformed`: lengths that are not whole vertices or triangles, or text
/// that is not UTF-8.
///
/// # Safety
///
/// Each non-null pointer points to its length in initialized bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_write_mesh(
    vertices: *const u8,
    vertices_len: usize,
    triangles: *const u8,
    triangles_len: usize,
    has_meta: u32,
    name: *const u8,
    name_len: usize,
    comment: *const u8,
    comment_len: usize,
) -> u32 {
    // SAFETY: the caller promises each length in readable bytes at its
    // pointer, and a null pointer only for an empty buffer or no text.
    let mesh = unsafe {
        mesh_from(
            (vertices, vertices_len),
            (triangles, triangles_len),
            has_meta,
            (name, name_len),
            (comment, comment_len),
        )
    };
    let refusal = match mesh {
        Some(mesh) => {
            WRITING.with_borrow_mut(|meshes| meshes.push(mesh));
            None
        }
        None => Some(Refusal {
            reason: "malformed",
            chunk: Some(MESH_TAG),
        }),
    };
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// The bytes of `(ptr, len)`, empty for a null `ptr`.
///
/// # Safety
///
/// A non-null `ptr` points to `len` initialized bytes.
unsafe fn bytes_at<'a>((ptr, len): (*const u8, usize)) -> &'a [u8] {
    if ptr.is_null() {
        return &[];
    }
    // SAFETY: the caller promises `len` readable bytes at a non-null `ptr`.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// A mesh from the buffers JavaScript gives [`bread_write_mesh`].
///
/// # Safety
///
/// As for [`bread_write_mesh`].
unsafe fn mesh_from(
    vertices: (*const u8, usize),
    triangles: (*const u8, usize),
    has_meta: u32,
    name: (*const u8, usize),
    comment: (*const u8, usize),
) -> Option<Mesh> {
    // SAFETY: as the caller promises.
    let (vertices, triangles) = unsafe { (bytes_at(vertices), bytes_at(triangles)) };
    let (vertices, rest) = vertices.as_chunks::<12>();
    let (triangles, more) = triangles.as_chunks::<12>();
    if !rest.is_empty() || !more.is_empty() {
        return None;
    }
    let words = |chunk: &[u8; 12]| -> [u32; 3] {
        std::array::from_fn(|i| {
            u32::from_le_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ])
        })
    };
    let meta = if has_meta == 1 {
        // SAFETY: as the caller promises.
        let (name, comment) = unsafe { (text_at(name.0, name.1), text_at(comment.0, comment.1)) };
        Some(Meta {
            name: name.ok()?,
            comment: comment.ok()?,
        })
    } else {
        None
    };
    Some(Mesh {
        meta,
        vertices: vertices
            .iter()
            .map(|c| words(c).map(f32::from_bits))
            .collect(),
        triangles: triangles.iter().map(words).collect(),
    })
}

/// Writes the file of the meshes added since [`bread_write_begin`], placed
/// by the scene chunk payload in the `len` bytes at `ptr`, as [`write_bread`]
/// does. Returns 1 when it was written, and the output accessors give it, and
/// 0 when it was refused. Either way the meshes added are dropped.
///
/// # Safety
///
/// As for [`bread_read_chunks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_write_file(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let payload = unsafe { std::slice::from_raw_parts(ptr, len) };
    let meshes = WRITING.take();
    let (output, refusal) = match write_bread(meshes, payload) {
        Ok(bytes) => (bytes, None),
        Err(refusal) => (Vec::new(), Some(refusal)),
    };
    OUTPUT.set(output);
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// Where the last file written is.
#[unsafe(no_mangle)]
pub extern "C" fn bread_output() -> *const u8 {
    OUTPUT.with_borrow(|output| output.as_ptr())
}

/// The length of the last file written.
#[unsafe(no_mangle)]
pub extern "C" fn bread_output_len() -> usize {
    OUTPUT.with_borrow(Vec::len)
}
