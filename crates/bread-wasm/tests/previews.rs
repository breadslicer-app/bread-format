/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `decode_previews_chunk` and its C ABI against the conformance files.

#![allow(unsafe_code)]

use std::path::PathBuf;

use bread_format::{Chunk, MESH_TAG, SCENE_TAG, SUPPORT_PREVIEW_TAG, read_file};
use bread_wasm::abi::{
    bread_alloc, bread_decode_previews, bread_free, bread_mesh_triangle_count,
    bread_mesh_vertex_count, bread_preview_count, bread_preview_leaf, bread_preview_select,
    bread_release,
};
use bread_wasm::{Refusal, decode_mesh_chunk, decode_previews_chunk, read_chunks};

fn conformance(path: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../conformance")
            .join(path),
    )
    .unwrap()
}

/// The preview chunk, the scene chunk, and which mesh chunks are empty.
fn parts(bytes: &[u8]) -> (Chunk, Chunk, Vec<bool>) {
    let chunks = read_chunks(bytes).unwrap();
    let find = |tag| chunks.iter().find(|c| c.tag == tag).unwrap().clone();
    let empty = chunks
        .iter()
        .filter(|c| c.tag == MESH_TAG)
        .map(|c| {
            let mesh = decode_mesh_chunk(c.version, &c.payload).unwrap();
            mesh.vertices.is_empty() && mesh.triangles.is_empty()
        })
        .collect();
    (find(SUPPORT_PREVIEW_TAG), find(SCENE_TAG), empty)
}

#[test]
fn the_previews_are_the_ones_the_crate_reads() {
    let bytes = conformance("valid/support-preview.bread");
    let (previews, scene, empty) = parts(&bytes);
    let decoded = decode_previews_chunk(
        previews.version,
        &previews.payload,
        scene.version,
        &scene.payload,
        &empty,
    )
    .unwrap();
    assert_eq!(
        decoded,
        read_file(bytes.as_slice()).unwrap().support_previews
    );
    assert!(!decoded.is_empty());
}

#[test]
fn a_preview_of_a_leaf_with_a_mesh_is_refused() {
    let (previews, scene, empty) = parts(&conformance("valid/support-preview.bread"));
    let full = vec![false; empty.len()];
    assert_eq!(
        decode_previews_chunk(
            previews.version,
            &previews.payload,
            scene.version,
            &scene.payload,
            &full
        ),
        Err(Refusal {
            reason: "malformed",
            chunk: Some(SUPPORT_PREVIEW_TAG),
        })
    );
    assert_eq!(
        decode_previews_chunk(2, &previews.payload, scene.version, &scene.payload, &empty),
        Err(Refusal {
            reason: "version",
            chunk: Some(SUPPORT_PREVIEW_TAG),
        })
    );
    let mut cut = previews.payload.clone();
    cut.pop();
    assert_eq!(
        decode_previews_chunk(
            previews.version,
            &cut,
            scene.version,
            &scene.payload,
            &empty
        ),
        Err(Refusal {
            reason: "malformed",
            chunk: Some(SUPPORT_PREVIEW_TAG),
        })
    );
}

/// Copies `bytes` in and hands `f` where they are; frees them after.
fn with_input<T>(bytes: &[u8], f: impl FnOnce(*const u8) -> T) -> T {
    let ptr = bread_alloc(bytes.len());
    // SAFETY: `ptr` holds `bytes.len()` bytes from `bread_alloc`, filled
    // here before `f` reads them, and freed once.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let out = f(ptr);
        bread_free(ptr, bytes.len());
        out
    }
}

#[test]
fn the_abi_gives_the_previews() {
    let bytes = conformance("valid/support-preview.bread");
    let (previews, scene, empty) = parts(&bytes);
    let empty: Vec<u8> = empty.iter().map(|&e| u8::from(e)).collect();
    let read = with_input(&previews.payload, |p| {
        with_input(&scene.payload, |s| {
            with_input(&empty, |e| {
                // SAFETY: each pointer holds its length in bytes.
                unsafe {
                    bread_decode_previews(
                        u32::from(previews.version),
                        p,
                        previews.payload.len(),
                        u32::from(scene.version),
                        s,
                        scene.payload.len(),
                        e,
                        empty.len(),
                    )
                }
            })
        })
    });
    assert_eq!(read, 1);
    let expected = read_file(bytes.as_slice()).unwrap().support_previews;
    assert_eq!(bread_preview_count(), expected.len());
    for (i, preview) in expected.iter().enumerate() {
        assert_eq!(bread_preview_leaf(i), preview.leaf);
        assert_eq!(bread_preview_select(i), 1);
        assert_eq!(bread_mesh_vertex_count(), preview.mesh.vertices.len());
        assert_eq!(bread_mesh_triangle_count(), preview.mesh.triangles.len());
    }
    assert_eq!(bread_preview_select(expected.len()), 0);
    assert_eq!(bread_mesh_vertex_count(), 0);
    bread_release();
    assert_eq!(bread_preview_count(), 0);
}
