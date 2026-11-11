/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `decode_mesh_chunk` and its C ABI against the conformance files.

#![allow(unsafe_code)]

use std::path::PathBuf;

use bread_format::{MESH_TAG, Mesh, read_file};
use bread_wasm::abi::{
    bread_alloc, bread_decode_mesh, bread_free, bread_mesh_has_meta, bread_mesh_text,
    bread_mesh_text_len, bread_mesh_triangle_count, bread_mesh_triangles, bread_mesh_vertex_count,
    bread_mesh_vertices, bread_refusal_chunk, bread_refusal_reason, bread_refusal_reason_len,
    bread_release,
};
use bread_wasm::{Refusal, decode_mesh_chunk, read_chunks};

fn conformance(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance")
        .join(path)
}

fn bread_files(dir: &str) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(conformance(dir))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bread"))
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

/// The mesh chunks of a file, decoded one by one.
fn meshes(bytes: &[u8]) -> Vec<Result<Mesh, Refusal>> {
    read_chunks(bytes)
        .unwrap()
        .into_iter()
        .filter(|chunk| chunk.tag == MESH_TAG)
        .map(|chunk| decode_mesh_chunk(chunk.version, &chunk.payload))
        .collect()
}

/// Bit for bit: `==` on `f32` would call NaN unequal and -0.0 equal to 0.0.
fn same(a: &Mesh, b: &Mesh) -> bool {
    a.meta == b.meta
        && a.triangles == b.triangles
        && a.vertices.len() == b.vertices.len()
        && a.vertices
            .iter()
            .zip(&b.vertices)
            .all(|(x, y)| (0..3).all(|i| x[i].to_bits() == y[i].to_bits()))
}

#[test]
fn every_valid_mesh_decodes_as_read_file_reads_it() {
    for (name, bytes) in bread_files("valid") {
        let file = read_file(bytes.as_slice()).unwrap();
        let decoded = meshes(&bytes);
        assert_eq!(decoded.len(), file.scene.meshes.len(), "{name}");
        for (got, want) in decoded.iter().zip(&file.scene.meshes) {
            let got = got.as_ref().unwrap_or_else(|err| panic!("{name}: {err:?}"));
            assert!(same(got, want), "{name}");
        }
    }
}

#[test]
fn the_mesh_cases_are_refused_for_their_reason() {
    let mut refused = 0;
    for (name, bytes) in bread_files("invalid") {
        let json = std::fs::read_to_string(conformance(&format!("invalid/{name}.json"))).unwrap();
        let mesh_case = json.contains("\"chunk\": \"MESH\"")
            && (json.contains("\"malformed\"") || json.contains("\"no-such-vertex\""));
        if !mesh_case {
            continue;
        }
        let reason = if json.contains("\"no-such-vertex\"") {
            "no-such-vertex"
        } else {
            "malformed"
        };
        let Ok(_) = read_chunks(&bytes) else {
            panic!("{name}: the container reads");
        };
        let err = meshes(&bytes)
            .into_iter()
            .find_map(Result::err)
            .unwrap_or_else(|| panic!("{name}: a mesh is refused"));
        assert_eq!(
            err,
            Refusal {
                reason,
                chunk: Some(MESH_TAG)
            },
            "{name}"
        );
        refused += 1;
    }
    // no-such-vertex, huge-vertex-count and the five mesh-* cases.
    assert_eq!(refused, 7);
}

#[test]
fn an_unknown_mesh_version_is_refused() {
    let (_, bytes) = bread_files("valid")
        .into_iter()
        .find(|(name, _)| name == "triangle")
        .unwrap();
    let chunk = read_chunks(&bytes)
        .unwrap()
        .into_iter()
        .find(|chunk| chunk.tag == MESH_TAG)
        .unwrap();
    for version in [0, 3] {
        assert_eq!(
            decode_mesh_chunk(version, &chunk.payload)
                .unwrap_err()
                .reason,
            "version"
        );
    }
}

/// Copies `payload` in and decodes it, as JavaScript does.
fn decode_through_abi(version: u32, payload: &[u8]) -> u32 {
    let ptr = bread_alloc(payload.len());
    // SAFETY: `ptr` holds `payload.len()` bytes from `bread_alloc`, filled
    // here before they are read, and freed once.
    unsafe {
        std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr, payload.len());
        let read = bread_decode_mesh(version, ptr, payload.len());
        bread_free(ptr, payload.len());
        read
    }
}

#[test]
fn the_abi_gives_the_mesh() {
    let bytes = std::fs::read(conformance("valid/cube.bread")).unwrap();
    let chunk = read_chunks(&bytes)
        .unwrap()
        .into_iter()
        .find(|chunk| chunk.tag == MESH_TAG)
        .unwrap();
    let want = decode_mesh_chunk(chunk.version, &chunk.payload).unwrap();
    assert_eq!(decode_through_abi(chunk.version.into(), &chunk.payload), 1);
    assert_eq!(bread_mesh_vertex_count(), want.vertices.len());
    assert_eq!(bread_mesh_triangle_count(), want.triangles.len());
    // SAFETY: the vertices and triangles of the last decoded mesh, still kept.
    let (vertices, triangles) = unsafe {
        (
            std::slice::from_raw_parts(bread_mesh_vertices(), 3 * want.vertices.len()),
            std::slice::from_raw_parts(bread_mesh_triangles(), 3 * want.triangles.len()),
        )
    };
    assert_eq!(vertices, want.vertices.as_flattened());
    assert_eq!(triangles, want.triangles.as_flattened());
    assert_eq!(bread_mesh_has_meta(), 1);
    // SAFETY: the name of the last decoded mesh, still kept.
    let name = unsafe { std::slice::from_raw_parts(bread_mesh_text(0), bread_mesh_text_len(0)) };
    assert_eq!(name, b"cube mesh");
    assert_eq!(bread_mesh_text_len(1), usize::MAX, "no comment");
    assert!(bread_mesh_text(1).is_null());
    assert!(bread_refusal_reason().is_null());

    bread_release();
    assert_eq!(bread_mesh_vertex_count(), 0);
    assert!(bread_mesh_vertices().is_null());
}

#[test]
fn the_abi_gives_the_refusal() {
    assert_eq!(decode_through_abi(2, &[0, 1, 2]), 0);
    assert_eq!(bread_mesh_vertex_count(), 0);
    // SAFETY: the reason of the last call, a static string.
    let reason =
        unsafe { std::slice::from_raw_parts(bread_refusal_reason(), bread_refusal_reason_len()) };
    assert_eq!(reason, b"malformed");
    assert_eq!(bread_refusal_chunk(), u32::from_le_bytes(*b"MESH"));
    assert_eq!(decode_through_abi(u32::MAX, &[]), 0, "a version past u16");
    bread_release();
}
