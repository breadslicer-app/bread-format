/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The C ABI the JavaScript package calls. JavaScript copies a file, or a
//! chunk's payload, into memory from [`bread_alloc`], calls
//! [`bread_read_chunks`], [`bread_decode_mesh`] or [`bread_decode_scene`],
//! frees the input, and copies out what it needs through the accessors.
//! Results are kept until the next call of the same kind, or
//! [`bread_release`].
//!
//! Indices and lists of indices come as `usize`, which is a `u32` in
//! WebAssembly.
//!
//! The accessors return 0, or a null pointer, for an index out of range or
//! when the last call was refused, so a mistake in JavaScript cannot read past
//! the results. Memory can grow on any call: JavaScript makes its views of it
//! again after each one.

#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};

use bread_format::{Chunk, Mesh, Meta, Node, NodeKind, Scene};

use crate::{Refusal, decode_mesh_chunk, decode_scene_chunk, read_chunks};

thread_local! {
    static CHUNKS: RefCell<Vec<Chunk>> = const { RefCell::new(Vec::new()) };
    pub(crate) static MESH: RefCell<Option<Mesh>> = const { RefCell::new(None) };
    static SCENE: RefCell<Option<DecodedScene>> = const { RefCell::new(None) };
    pub(crate) static REFUSAL: Cell<Option<Refusal>> = const { Cell::new(None) };
}

/// A decoded scene, with each node's transform laid out as JavaScript reads
/// it: the nine numbers of the linear part row by row, then the translation.
#[derive(Debug)]
struct DecodedScene {
    scene: Scene,
    transforms: Vec<Option<[f64; 12]>>,
}

impl DecodedScene {
    fn new(scene: Scene) -> Self {
        let transforms = scene
            .nodes
            .iter()
            .map(|node| {
                node.transform.map(|transform| {
                    let mut flat = [0.0; 12];
                    for (out, value) in flat.iter_mut().zip(
                        transform
                            .linear
                            .as_flattened()
                            .iter()
                            .chain(&transform.translation),
                    ) {
                        *out = *value;
                    }
                    flat
                })
            })
            .collect();
        Self { scene, transforms }
    }
}

fn with_scene<T>(none: T, f: impl FnOnce(&DecodedScene) -> T) -> T {
    SCENE.with_borrow(|scene| scene.as_ref().map_or(none, f))
}

fn with_node<T>(index: usize, none: T, f: impl FnOnce(&Node) -> T) -> T {
    SCENE.with_borrow(|scene| {
        scene
            .as_ref()
            .and_then(|decoded| decoded.scene.nodes.get(index))
            .map_or(none, f)
    })
}

fn with_chunk<T>(index: usize, none: T, f: impl FnOnce(&Chunk) -> T) -> T {
    CHUNKS.with_borrow(|chunks| chunks.get(index).map_or(none, f))
}

fn with_mesh<T>(none: T, f: impl FnOnce(&Mesh) -> T) -> T {
    MESH.with_borrow(|mesh| mesh.as_ref().map_or(none, f))
}

fn with_refusal<T>(none: T, f: impl FnOnce(&Refusal) -> T) -> T {
    REFUSAL.get().as_ref().map_or(none, f)
}

/// Reserves `len` bytes for JavaScript to fill, and returns where they are.
#[unsafe(no_mangle)]
pub extern "C" fn bread_alloc(len: usize) -> *mut u8 {
    let mut bytes = Vec::<u8>::with_capacity(len);
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

/// Frees what [`bread_alloc`] reserved.
///
/// # Safety
///
/// `ptr` and `len` come from one call to [`bread_alloc`], not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_free(ptr: *mut u8, len: usize) {
    // SAFETY: the caller hands back an allocation of `len` bytes from
    // `bread_alloc`, which made it as a `Vec<u8>` of that capacity.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len) });
}

/// Reads the chunks of the `len` bytes at `ptr`, as [`read_chunks`] does.
/// Returns 1 when the file was read and 0 when it was refused.
///
/// # Safety
///
/// `ptr` points to `len` initialized bytes, such as those from
/// [`bread_alloc`] once JavaScript has filled them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_read_chunks(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let (chunks, refusal) = match read_chunks(bytes) {
        Ok(chunks) => (chunks, None),
        Err(refusal) => (Vec::new(), Some(refusal)),
    };
    CHUNKS.set(chunks);
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// Decodes the mesh chunk payload of `version` in the `len` bytes at `ptr`,
/// as [`decode_mesh_chunk`] does. Returns 1 when the mesh was read and 0 when
/// it was refused.
///
/// # Safety
///
/// As for [`bread_read_chunks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_decode_mesh(version: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let payload = unsafe { std::slice::from_raw_parts(ptr, len) };
    // A version past u16 is no version the format has: decode_mesh_chunk
    // refuses u16::MAX like any other unknown one.
    let version = u16::try_from(version).unwrap_or(u16::MAX);
    let (mesh, refusal) = match decode_mesh_chunk(version, payload) {
        Ok(mesh) => (Some(mesh), None),
        Err(refusal) => (None, Some(refusal)),
    };
    MESH.set(mesh);
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// Decodes the scene chunk payload of `version` in the `len` bytes at `ptr`,
/// for a file of `mesh_count` mesh chunks, as [`decode_scene_chunk`] does.
/// Returns 1 when the scene was read and 0 when it was refused.
///
/// # Safety
///
/// As for [`bread_read_chunks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_decode_scene(
    version: u32,
    mesh_count: usize,
    ptr: *const u8,
    len: usize,
) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let payload = unsafe { std::slice::from_raw_parts(ptr, len) };
    let version = u16::try_from(version).unwrap_or(u16::MAX);
    let (scene, refusal) = match decode_scene_chunk(version, payload, mesh_count) {
        Ok(scene) => (Some(DecodedScene::new(scene)), None),
        Err(refusal) => (None, Some(refusal)),
    };
    SCENE.set(scene);
    REFUSAL.set(refusal);
    u32::from(refusal.is_none())
}

/// Drops every result kept, and the file being written.
#[unsafe(no_mangle)]
pub extern "C" fn bread_release() {
    CHUNKS.set(Vec::new());
    MESH.set(None);
    SCENE.set(None);
    REFUSAL.set(None);
    #[cfg(feature = "write")]
    crate::write_abi::release();
}

/// How many chunks the last read returned.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_count() -> usize {
    CHUNKS.with_borrow(Vec::len)
}

/// The tag of chunk `index`: its four bytes read as a little-endian `u32`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_tag(index: usize) -> u32 {
    with_chunk(index, 0, |chunk| u32::from_le_bytes(chunk.tag.0))
}

/// The version of chunk `index`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_version(index: usize) -> u32 {
    with_chunk(index, 0, |chunk| u32::from(chunk.version))
}

/// Where the payload of chunk `index` is.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_payload(index: usize) -> *const u8 {
    with_chunk(index, std::ptr::null(), |chunk| chunk.payload.as_ptr())
}

/// The length of the payload of chunk `index`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_payload_len(index: usize) -> usize {
    with_chunk(index, 0, |chunk| chunk.payload.len())
}

/// How many vertices the last decoded mesh has.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_vertex_count() -> usize {
    with_mesh(0, |mesh| mesh.vertices.len())
}

/// Where the vertices of the last decoded mesh are: x, y and z of each, as
/// `f32`s.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_vertices() -> *const f32 {
    with_mesh(std::ptr::null(), |mesh| mesh.vertices.as_ptr().cast())
}

/// How many triangles the last decoded mesh has.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_triangle_count() -> usize {
    with_mesh(0, |mesh| mesh.triangles.len())
}

/// Where the triangles of the last decoded mesh are: the three vertex
/// indices of each, as `u32`s, first corner first.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_triangles() -> *const u32 {
    with_mesh(std::ptr::null(), |mesh| mesh.triangles.as_ptr().cast())
}

/// 1 when the last decoded mesh has metadata, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_has_meta() -> u32 {
    with_mesh(0, |mesh| u32::from(mesh.meta.is_some()))
}

/// The name (`which` = 0) or comment (1) of `meta`, when it has one.
fn meta_text(meta: Option<&Meta>, which: u32) -> Option<&str> {
    match which {
        0 => meta.and_then(|meta| meta.name.as_deref()),
        1 => meta.and_then(|meta| meta.comment.as_deref()),
        _ => None,
    }
}

/// The mesh's name (`which` = 0) or comment (1), when it has one.
fn mesh_text<T>(which: u32, none: T, f: impl FnOnce(&str) -> T) -> T {
    with_mesh(None, |mesh| meta_text(mesh.meta.as_ref(), which).map(f)).unwrap_or(none)
}

/// Where the last decoded mesh's name (`which` = 0) or comment (1) is, in
/// UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_text(which: u32) -> *const u8 {
    mesh_text(which, std::ptr::null(), str::as_ptr)
}

/// The length of the last decoded mesh's name (`which` = 0) or comment (1),
/// or `usize::MAX` when it has none.
#[unsafe(no_mangle)]
pub extern "C" fn bread_mesh_text_len(which: u32) -> usize {
    mesh_text(which, usize::MAX, str::len)
}

/// The node a scene accessor names by `target`: [`SCENE_ITSELF`] for the
/// scene, else a node index.
pub const SCENE_ITSELF: usize = usize::MAX;

/// `f` of the metadata of `target` in the last decoded scene, or `none` when
/// there is no such target.
fn scene_meta<T>(target: usize, none: T, f: impl FnOnce(Option<&Meta>) -> T) -> T {
    with_scene(None, |decoded| {
        if target == SCENE_ITSELF {
            Some(decoded.scene.meta.as_ref())
        } else {
            decoded
                .scene
                .nodes
                .get(target)
                .map(|node| node.meta.as_ref())
        }
        .map(f)
    })
    .unwrap_or(none)
}

/// 1 when `target` of the last decoded scene, the scene itself or a node,
/// has metadata, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_has_meta(target: usize) -> u32 {
    scene_meta(target, 0, |meta| u32::from(meta.is_some()))
}

/// Where the name (`which` = 0) or comment (1) of `target` of the last
/// decoded scene is, in UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_text(target: usize, which: u32) -> *const u8 {
    scene_meta(target, std::ptr::null(), |meta| {
        meta_text(meta, which).map_or(std::ptr::null(), str::as_ptr)
    })
}

/// The length of the name (`which` = 0) or comment (1) of `target` of the
/// last decoded scene, or `usize::MAX` when it has none.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_text_len(target: usize, which: u32) -> usize {
    scene_meta(target, usize::MAX, |meta| {
        meta_text(meta, which).map_or(usize::MAX, str::len)
    })
}

/// How many nodes the last decoded scene has.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_count() -> usize {
    with_scene(0, |decoded| decoded.scene.nodes.len())
}

/// The kind of node `index`, as its record gives it: 0 group, 1 leaf, 2 leaf
/// with a shared mesh, 4 reference; `u32::MAX` when there is no such node.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_kind(index: usize) -> u32 {
    with_node(index, u32::MAX, |node| match node.kind {
        NodeKind::Group { .. } => 0,
        NodeKind::Leaf { .. } => 1,
        NodeKind::SharedLeaf { .. } => 2,
        NodeKind::Reference { .. } => 4,
    })
}

/// What node `index` names: the mesh of a leaf, the mesh resource of a leaf
/// with a shared mesh, or the subtree of a reference; 0 for a group.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_target(index: usize) -> usize {
    with_node(index, 0, |node| match node.kind {
        NodeKind::Group { .. } => 0,
        NodeKind::Leaf { mesh } => mesh,
        NodeKind::SharedLeaf { resource } => resource,
        NodeKind::Reference { subtree } => subtree,
    })
}

/// Where the children of node `index` are, as node indices, when it is a
/// group.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_children(index: usize) -> *const usize {
    with_node(index, std::ptr::null(), |node| match &node.kind {
        NodeKind::Group { children } => children.as_ptr(),
        _ => std::ptr::null(),
    })
}

/// How many children node `index` has: 0 when it is not a group.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_children_len(index: usize) -> usize {
    with_node(index, 0, |node| match &node.kind {
        NodeKind::Group { children } => children.len(),
        _ => 0,
    })
}

/// Where the transform of node `index` is: twelve `f64`, the linear part row
/// by row and then the translation; null for a node stored without one.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_node_transform(index: usize) -> *const f64 {
    with_scene(std::ptr::null(), |decoded| {
        decoded
            .transforms
            .get(index)
            .and_then(Option::as_ref)
            .map_or(std::ptr::null(), |flat| flat.as_ptr())
    })
}

/// One of the last decoded scene's lists of indices: its mesh resources
/// (`which` = 0), the root nodes of its subtrees (1) or its roots (2).
fn scene_list<T>(which: u32, none: T, f: impl FnOnce(&[usize]) -> T) -> T {
    with_scene(None, |decoded| {
        let scene = &decoded.scene;
        match which {
            0 => Some(f(&scene.mesh_resources)),
            1 => Some(f(&scene.subtrees)),
            2 => Some(f(&scene.roots)),
            _ => None,
        }
    })
    .unwrap_or(none)
}

/// Where the mesh resources (`which` = 0), the subtrees' root nodes (1) or
/// the roots (2) of the last decoded scene are.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_list(which: u32) -> *const usize {
    scene_list(which, std::ptr::null(), <[usize]>::as_ptr)
}

/// How many entries that list has.
#[unsafe(no_mangle)]
pub extern "C" fn bread_scene_list_len(which: u32) -> usize {
    scene_list(which, 0, <[usize]>::len)
}

/// Where the reason code of the last refusal is, in UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_reason() -> *const u8 {
    with_refusal(std::ptr::null(), |refusal| refusal.reason.as_ptr())
}

/// The length of the reason code of the last refusal.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_reason_len() -> usize {
    with_refusal(0, |refusal| refusal.reason.len())
}

/// The tag of the chunk the last refusal is about, as [`bread_chunk_tag`]
/// gives it, or 0 when it is about the whole file.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_chunk() -> u32 {
    with_refusal(0, |refusal| {
        refusal.chunk.map_or(0, |tag| u32::from_le_bytes(tag.0))
    })
}
