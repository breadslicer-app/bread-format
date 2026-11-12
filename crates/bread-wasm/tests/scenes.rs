/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `decode_scene_chunk` and its C ABI against the conformance files.

#![allow(unsafe_code)]

use std::path::PathBuf;

use bread_format::{
    Chunk, MESH_TAG, Mesh, Meta, Node, NodeKind, SCENE_TAG, Scene, Transform, encode_scene,
    read_file,
};
use bread_wasm::abi::{
    SCENE_ITSELF, bread_alloc, bread_decode_scene, bread_free, bread_refusal_chunk,
    bread_refusal_reason, bread_refusal_reason_len, bread_release, bread_scene_has_meta,
    bread_scene_list, bread_scene_list_len, bread_scene_node_children,
    bread_scene_node_children_len, bread_scene_node_count, bread_scene_node_kind,
    bread_scene_node_target, bread_scene_node_transform, bread_scene_text, bread_scene_text_len,
};
use bread_wasm::{Refusal, decode_scene_chunk, read_chunks};

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

/// The scene chunk of a file, and how many mesh chunks it holds.
fn scene_chunk(bytes: &[u8]) -> (Chunk, usize) {
    let chunks = read_chunks(bytes).unwrap();
    let meshes = chunks.iter().filter(|chunk| chunk.tag == MESH_TAG).count();
    let scene = chunks
        .into_iter()
        .find(|chunk| chunk.tag == SCENE_TAG)
        .unwrap();
    (scene, meshes)
}

/// The scene without its meshes, which `decode_scene_chunk` does not hold.
fn without_meshes(scene: Scene) -> Scene {
    Scene {
        meshes: vec![Mesh::default(); scene.meshes.len()],
        ..scene
    }
}

#[test]
fn every_valid_scene_is_the_one_the_crate_reads() {
    for (name, bytes) in bread_files("valid") {
        let (chunk, meshes) = scene_chunk(&bytes);
        let scene = decode_scene_chunk(chunk.version, &chunk.payload, meshes)
            .unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let expected = without_meshes(read_file(bytes.as_slice()).unwrap().scene);
        assert_eq!(scene, expected, "{name}");
    }
}

#[test]
fn a_leaf_that_names_no_mesh_is_malformed() {
    let bytes = std::fs::read(conformance("invalid/no-such-mesh.bread")).unwrap();
    let (chunk, meshes) = scene_chunk(&bytes);
    assert_eq!(
        decode_scene_chunk(chunk.version, &chunk.payload, meshes),
        Err(Refusal {
            reason: "malformed",
            chunk: Some(SCENE_TAG),
        })
    );
}

#[test]
fn a_scene_that_names_another_mesh_count_is_malformed() {
    let bytes = std::fs::read(conformance("valid/two-parts.bread")).unwrap();
    let (chunk, meshes) = scene_chunk(&bytes);
    for count in [meshes - 1, meshes + 1] {
        assert_eq!(
            decode_scene_chunk(chunk.version, &chunk.payload, count),
            Err(Refusal {
                reason: "malformed",
                chunk: Some(SCENE_TAG),
            }),
            "{count} mesh chunks"
        );
    }
}

#[test]
fn an_unknown_scene_version_is_refused() {
    let bytes = std::fs::read(conformance("valid/triangle.bread")).unwrap();
    let (chunk, meshes) = scene_chunk(&bytes);
    for version in [0, 2] {
        assert_eq!(
            decode_scene_chunk(version, &chunk.payload, meshes),
            Err(Refusal {
                reason: "version",
                chunk: Some(SCENE_TAG),
            })
        );
    }
}

/// Copies `payload` in and decodes it, as JavaScript does.
fn decode_through_abi(version: u32, payload: &[u8], meshes: usize) -> u32 {
    let ptr = bread_alloc(payload.len());
    // SAFETY: `ptr` holds `payload.len()` bytes from `bread_alloc`, filled
    // here before they are read, and freed once.
    unsafe {
        std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr, payload.len());
        let read = bread_decode_scene(version, meshes, ptr, payload.len());
        bread_free(ptr, payload.len());
        read
    }
}

fn text(target: usize, which: u32) -> Option<String> {
    let len = bread_scene_text_len(target, which);
    if len == usize::MAX {
        assert!(bread_scene_text(target, which).is_null());
        return None;
    }
    // SAFETY: text of the last decoded scene, still kept.
    let bytes = unsafe { std::slice::from_raw_parts(bread_scene_text(target, which), len) };
    Some(String::from_utf8(bytes.to_vec()).unwrap())
}

fn meta_through_abi(target: usize) -> Option<Meta> {
    (bread_scene_has_meta(target) == 1).then(|| Meta {
        name: text(target, 0),
        comment: text(target, 1),
    })
}

fn list(which: u32) -> Vec<usize> {
    let len = bread_scene_list_len(which);
    if len == 0 {
        return Vec::new();
    }
    // SAFETY: a list of the last decoded scene, still kept.
    unsafe { std::slice::from_raw_parts(bread_scene_list(which), len) }.to_vec()
}

/// The scene as JavaScript reads it back through the accessors.
fn scene_through_abi(meshes: usize) -> Scene {
    let nodes = (0..bread_scene_node_count())
        .map(|index| {
            let target = bread_scene_node_target(index);
            let kind = match bread_scene_node_kind(index) {
                0 => {
                    let len = bread_scene_node_children_len(index);
                    let children = if len == 0 {
                        Vec::new()
                    } else {
                        // SAFETY: the children of a node of the last decoded
                        // scene, still kept.
                        unsafe { std::slice::from_raw_parts(bread_scene_node_children(index), len) }
                            .to_vec()
                    };
                    NodeKind::Group { children }
                }
                1 => NodeKind::Leaf { mesh: target },
                2 => NodeKind::SharedLeaf { resource: target },
                4 => NodeKind::Reference { subtree: target },
                other => panic!("node {index} has kind {other}"),
            };
            let ptr = bread_scene_node_transform(index);
            let transform = (!ptr.is_null()).then(|| {
                // SAFETY: twelve f64 of the last decoded scene, still kept.
                let flat = unsafe { std::slice::from_raw_parts(ptr, 12) };
                Transform {
                    linear: [
                        [flat[0], flat[1], flat[2]],
                        [flat[3], flat[4], flat[5]],
                        [flat[6], flat[7], flat[8]],
                    ],
                    translation: [flat[9], flat[10], flat[11]],
                }
            });
            Node {
                meta: meta_through_abi(index),
                transform,
                kind,
            }
        })
        .collect();
    Scene {
        meta: meta_through_abi(SCENE_ITSELF),
        meshes: vec![Mesh::default(); meshes],
        mesh_resources: list(0),
        nodes,
        subtrees: list(1),
        roots: list(2),
    }
}

/// A scene with every kind of node, metadata with and without a comment,
/// transforms and identities, mesh resources, and a subtree that two
/// references copy.
fn every_kind() -> Scene {
    let named = |name: &str| {
        Some(Meta {
            name: Some(name.to_owned()),
            comment: None,
        })
    };
    let moved = |x: f64| {
        Some(Transform {
            linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
            translation: [x, 0.25, -1.5],
        })
    };
    Scene {
        meta: Some(Meta {
            name: Some("kit".to_owned()),
            comment: Some("every kind of node".to_owned()),
        }),
        meshes: vec![Mesh::default(); 2],
        mesh_resources: vec![1],
        nodes: vec![
            Node {
                meta: named("arm"),
                transform: moved(1.0),
                kind: NodeKind::Leaf { mesh: 0 },
            },
            Node {
                meta: None,
                transform: None,
                kind: NodeKind::SharedLeaf { resource: 0 },
            },
            Node {
                meta: named("limb"),
                transform: None,
                kind: NodeKind::Group {
                    children: vec![0, 1],
                },
            },
            Node {
                meta: named("left"),
                transform: moved(-10.0),
                kind: NodeKind::Reference { subtree: 0 },
            },
            Node {
                meta: named("right"),
                transform: moved(10.0),
                kind: NodeKind::Reference { subtree: 0 },
            },
            Node {
                meta: None,
                transform: None,
                kind: NodeKind::Group {
                    children: vec![3, 4],
                },
            },
        ],
        subtrees: vec![2],
        roots: vec![5],
    }
}

#[test]
fn the_abi_gives_the_scene() {
    let scene = every_kind();
    let payload = encode_scene(&scene).unwrap();
    assert_eq!(decode_through_abi(1, &payload, 2), 1);
    assert_eq!(scene_through_abi(2), scene);

    let past = scene.nodes.len();
    assert_eq!(bread_scene_node_kind(past), u32::MAX);
    assert!(bread_scene_node_transform(past).is_null());
    assert!(bread_scene_node_children(past).is_null());
    assert_eq!(bread_scene_has_meta(past), 0);
    assert_eq!(bread_scene_list_len(3), 0);
    assert!(bread_refusal_reason().is_null());

    bread_release();
    assert_eq!(bread_scene_node_count(), 0);
    assert_eq!(bread_scene_has_meta(SCENE_ITSELF), 0);
}

#[test]
fn the_abi_gives_every_valid_scene() {
    for (name, bytes) in bread_files("valid") {
        let (chunk, meshes) = scene_chunk(&bytes);
        assert_eq!(
            decode_through_abi(u32::from(chunk.version), &chunk.payload, meshes),
            1,
            "{name}"
        );
        let expected = without_meshes(read_file(bytes.as_slice()).unwrap().scene);
        assert_eq!(scene_through_abi(meshes), expected, "{name}");
    }
    bread_release();
}

#[test]
fn the_abi_gives_the_refusal() {
    let bytes = std::fs::read(conformance("invalid/no-such-mesh.bread")).unwrap();
    let (chunk, meshes) = scene_chunk(&bytes);
    assert_eq!(
        decode_through_abi(u32::from(chunk.version), &chunk.payload, meshes),
        0
    );
    assert_eq!(bread_scene_node_count(), 0);
    let len = bread_refusal_reason_len();
    // SAFETY: the reason of the last call, a static string.
    let reason = unsafe { std::slice::from_raw_parts(bread_refusal_reason(), len) };
    assert_eq!(reason, b"malformed");
    assert_eq!(bread_refusal_chunk(), u32::from_le_bytes(*b"SCNE"));
    bread_release();
}
