/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `read_stl_mesh`, `write_bread` and their C ABI, built with the `write`
//! feature.

#![cfg(feature = "write")]
#![allow(unsafe_code)]

use bread_format::{
    MESH_TAG, Mesh, Meta, Node, NodeKind, SCENE_TAG, Scene, Transform, encode_scene, read_file,
};
use bread_wasm::abi::{
    bread_alloc, bread_free, bread_mesh_triangle_count, bread_mesh_vertex_count,
    bread_refusal_reason, bread_refusal_reason_len, bread_release,
};
use bread_wasm::write_abi::{
    bread_output, bread_output_len, bread_read_stl, bread_write_begin, bread_write_file,
    bread_write_mesh,
};
use bread_wasm::{Refusal, read_stl_mesh, write_bread};

fn named(name: &str) -> Option<Meta> {
    Some(Meta {
        name: Some(name.to_owned()),
        comment: None,
    })
}

fn triangle(z: f32) -> Mesh {
    Mesh {
        meta: named("triangle"),
        vertices: vec![[0.0, 0.0, z], [1.0, 0.0, z], [0.0, -0.0, f32::from_bits(1)]],
        triangles: vec![[0, 1, 2]],
    }
}

/// Two meshes, each one leaf, the second moved.
fn kit() -> Scene {
    Scene {
        meta: named("kit"),
        meshes: vec![triangle(0.0), triangle(5.0)],
        mesh_resources: Vec::new(),
        nodes: vec![
            Node {
                meta: named("a.stl"),
                transform: None,
                kind: NodeKind::Leaf { mesh: 0 },
            },
            Node {
                meta: named("b.stl"),
                transform: Some(Transform {
                    linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    translation: [10.0, 0.0, 0.0],
                }),
                kind: NodeKind::Leaf { mesh: 1 },
            },
        ],
        subtrees: Vec::new(),
        roots: vec![0, 1],
    }
}

#[test]
fn a_written_file_reads_back_the_same() {
    let scene = kit();
    let bytes = write_bread(scene.meshes.clone(), &encode_scene(&scene).unwrap()).unwrap();
    assert_eq!(read_file(bytes.as_slice()).unwrap().scene, scene);
}

#[test]
fn a_scene_a_reader_would_refuse_is_not_written() {
    let mut scene = kit();
    let payload = encode_scene(&scene).unwrap();
    // One mesh fewer than the scene names.
    scene.meshes.pop();
    assert_eq!(
        write_bread(scene.meshes, &payload),
        Err(Refusal {
            reason: "malformed",
            chunk: Some(SCENE_TAG),
        })
    );
    assert_eq!(
        write_bread(Vec::new(), b"\x07"),
        Err(Refusal {
            reason: "malformed",
            chunk: Some(SCENE_TAG),
        })
    );
}

#[test]
fn a_triangle_past_the_vertices_is_not_written() {
    let mut scene = kit();
    scene.meshes[1].triangles[0][2] = 3;
    assert_eq!(
        write_bread(scene.meshes.clone(), &encode_scene(&scene).unwrap()),
        Err(Refusal {
            reason: "no-such-vertex",
            chunk: Some(MESH_TAG),
        })
    );
}

#[test]
fn an_stl_is_read_or_refused() {
    let mut stl = vec![0; 80];
    stl.extend_from_slice(&1u32.to_le_bytes());
    stl.extend_from_slice(&[0; 12]);
    for corner in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in corner {
            stl.extend_from_slice(&value.to_le_bytes());
        }
    }
    stl.extend_from_slice(&[0; 2]);
    let mesh = read_stl_mesh(&stl).unwrap();
    assert_eq!(mesh.triangles, [[0, 1, 2]]);

    let refused = |reason| {
        Err(Refusal {
            reason,
            chunk: None,
        })
    };
    assert_eq!(read_stl_mesh(b"not an stl"), refused("not-stl"));
    assert_eq!(
        read_stl_mesh(b"solid\nvertex 1 2 3\nendsolid\n"),
        refused("bad-stl")
    );
}

/// Copies `bytes` in, calls `f` with where they are, and frees them.
fn with_input<T>(bytes: &[u8], f: impl FnOnce(*const u8, usize) -> T) -> T {
    let ptr = bread_alloc(bytes.len());
    // SAFETY: `ptr` holds `bytes.len()` bytes from `bread_alloc`, filled
    // here before `f` reads them, and freed once.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let out = f(ptr, bytes.len());
        bread_free(ptr, bytes.len());
        out
    }
}

fn le_bytes(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}

/// Adds `mesh` through the ABI, as JavaScript does.
fn add_mesh(mesh: &Mesh) -> u32 {
    let vertices = le_bytes(mesh.vertices.as_flattened().iter().map(|v| v.to_bits()));
    let triangles = le_bytes(mesh.triangles.as_flattened().iter().copied());
    let meta = mesh.meta.as_ref();
    let name = meta.and_then(|m| m.name.clone());
    let comment = meta.and_then(|m| m.comment.clone());
    with_input(&vertices, |v, v_len| {
        with_input(&triangles, |t, t_len| {
            let text = |s: &Option<String>| {
                s.as_ref()
                    .map_or((std::ptr::null(), 0), |s| (s.as_ptr(), s.len()))
            };
            let (name, name_len) = text(&name);
            let (comment, comment_len) = text(&comment);
            // SAFETY: every pointer is null or points to its length in
            // initialized bytes, kept alive across the call.
            unsafe {
                bread_write_mesh(
                    v,
                    v_len,
                    t,
                    t_len,
                    u32::from(meta.is_some()),
                    name,
                    name_len,
                    comment,
                    comment_len,
                )
            }
        })
    })
}

fn refusal_reason() -> Vec<u8> {
    let len = bread_refusal_reason_len();
    // SAFETY: the reason of the last call, a static string.
    unsafe { std::slice::from_raw_parts(bread_refusal_reason(), len) }.to_vec()
}

#[test]
fn the_abi_writes_a_file() {
    let scene = kit();
    bread_write_begin();
    for mesh in &scene.meshes {
        assert_eq!(add_mesh(mesh), 1);
    }
    let payload = encode_scene(&scene).unwrap();
    // SAFETY: the payload's bytes, as `with_input` hands them over.
    let written = with_input(&payload, |ptr, len| unsafe { bread_write_file(ptr, len) });
    assert_eq!(written, 1);
    // SAFETY: the output of the last write, still kept.
    let bytes = unsafe { std::slice::from_raw_parts(bread_output(), bread_output_len()) };
    assert_eq!(read_file(bytes).unwrap().scene, scene);

    // The meshes were used up: the same scene again names two that are not
    // there.
    // SAFETY: as above.
    let again = with_input(&payload, |ptr, len| unsafe { bread_write_file(ptr, len) });
    assert_eq!(again, 0);
    assert_eq!(refusal_reason(), b"malformed");
    assert_eq!(bread_output_len(), 0);
    bread_release();
}

#[test]
fn the_abi_refuses_a_mesh_of_partial_vertices() {
    bread_write_begin();
    // SAFETY: eleven bytes of vertices, and no triangles or text.
    let added = with_input(&[0; 11], |ptr, len| unsafe {
        bread_write_mesh(
            ptr,
            len,
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
        )
    });
    assert_eq!(added, 0);
    assert_eq!(refusal_reason(), b"malformed");

    // SAFETY: as above, with a name that is not UTF-8.
    let added = with_input(&[0xff], |name, len| unsafe {
        bread_write_mesh(
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            1,
            name,
            len,
            std::ptr::null(),
            0,
        )
    });
    assert_eq!(added, 0);
    bread_release();
}

#[test]
fn the_abi_reads_an_stl_into_the_mesh_accessors() {
    let text = b"solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n";
    // SAFETY: the STL's bytes, as `with_input` hands them over.
    assert_eq!(
        with_input(text, |ptr, len| unsafe { bread_read_stl(ptr, len) }),
        1
    );
    assert_eq!(bread_mesh_vertex_count(), 3);
    assert_eq!(bread_mesh_triangle_count(), 1);
    // SAFETY: as above.
    assert_eq!(
        with_input(b"nope", |ptr, len| unsafe { bread_read_stl(ptr, len) }),
        0
    );
    assert_eq!(refusal_reason(), b"not-stl");
    bread_release();
}
