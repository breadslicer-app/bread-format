/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use bread_format::{BreadFile, Mesh, Node, NodeKind, Scene, Transform, read_file, read_stl};

use super::{PLACEMENT_LIMIT, StlFile, from_stl, grouped, place, to_stl};

/// A binary STL of these triangles, each three corners.
fn binary_stl(triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut out = b"a test".to_vec();
    out.resize(80, 0);
    out.extend_from_slice(&u32::try_from(triangles.len()).unwrap().to_le_bytes());
    for t in triangles {
        out.extend_from_slice(&[0; 12]);
        for corner in t {
            for x in corner {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0; 2]);
    }
    out
}

/// Two triangles sharing an edge, with a -0.0 and an odd value, so bits
/// matter.
fn quad() -> Vec<[[f32; 3]; 3]> {
    let a = [-0.0, 0.0, 0.1];
    let b = [1.0, 0.0, 0.1];
    let c = [1.0, 1.0, 1.0e-30];
    let d = [0.0, 1.0, 0.1];
    vec![[a, b, c], [a, c, d]]
}

fn corners(stl: &[u8]) -> Vec<[u32; 3]> {
    let count = u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize;
    assert_eq!(stl.len(), 84 + 50 * count);
    let mut out = Vec::new();
    for t in 0..count {
        for c in 0..3 {
            let at = 84 + 50 * t + 12 + 12 * c;
            out.push(
                [0, 1, 2].map(|k| {
                    u32::from_le_bytes(stl[at + 4 * k..at + 4 * k + 4].try_into().unwrap())
                }),
            );
        }
    }
    out
}

#[test]
fn stl_files_become_one_file_a_part_each() {
    let one = binary_stl(&quad());
    let two = binary_stl(&quad()[..1]);
    let files = [
        StlFile {
            name: "a.stl".into(),
            bytes: one.clone(),
        },
        StlFile {
            name: "b.stl".into(),
            bytes: two,
        },
    ];
    let (bytes, parts) = from_stl(&files, Some("kit")).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].vertices, parts[0].triangles), (4, 2));
    let file = read_file(bytes.as_slice()).unwrap();
    let names: Vec<_> = file
        .scene
        .nodes
        .iter()
        .map(|n| n.meta.as_ref().unwrap().name.as_deref().unwrap())
        .collect();
    assert_eq!(names, ["a.stl", "b.stl"]);
    assert_eq!(
        file.scene.meta.as_ref().unwrap().name.as_deref(),
        Some("kit")
    );
    // And back: the same corners, bit for bit, in order.
    let only_a = BreadFile {
        scene: Scene {
            nodes: file.scene.nodes[..1].to_vec(),
            roots: vec![0],
            ..file.scene.clone()
        },
        ..file.clone()
    };
    let (stl, shown) = to_stl(&only_a).unwrap();
    assert_eq!(shown, 1);
    assert_eq!(corners(&stl), corners(&one));
}

#[test]
fn ascii_stl_is_read_too() {
    let text = b"solid part\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid part\n";
    let (bytes, parts) = from_stl(
        &[StlFile {
            name: "part.stl".into(),
            bytes: text.to_vec(),
        }],
        None,
    )
    .unwrap();
    assert_eq!(parts[0].triangles, 1);
    let file = read_file(bytes.as_slice()).unwrap();
    assert_eq!(file.scene.meshes[0], read_stl(text).unwrap());
}

#[test]
fn what_is_not_stl_is_refused_with_its_name() {
    let err = from_stl(
        &[StlFile {
            name: "notes.stl".into(),
            bytes: b"hello".to_vec(),
        }],
        None,
    )
    .unwrap_err();
    assert!(err.0.starts_with("notes.stl: "), "{err}");
    assert!(from_stl(&[], None).is_err());
}

fn leaf(mesh: usize, transform: Option<Transform>) -> Node {
    Node {
        meta: None,
        transform,
        kind: NodeKind::Leaf { mesh },
    }
}

fn triangle() -> Mesh {
    Mesh {
        meta: None,
        vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        triangles: vec![[0, 1, 2]],
    }
}

#[test]
fn parts_go_to_their_world_positions_and_mirrors_keep_facing_out() {
    let moved = Transform {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [10.0, 0.0, 0.0],
    };
    let mirrored = Transform {
        linear: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 0.0, 0.0],
    };
    let file = BreadFile {
        scene: Scene {
            meshes: vec![triangle()],
            nodes: vec![leaf(0, Some(moved)), leaf(0, Some(mirrored))],
            roots: vec![0, 1],
            ..Scene::default()
        },
        ..BreadFile::default()
    };
    let (stl, parts) = to_stl(&file).unwrap();
    assert_eq!(parts, 2);
    let c = corners(&stl);
    let f = |x: f32| x.to_bits();
    assert_eq!(c[1], [f(11.0), f(0.0), f(0.0)]);
    // Mirrored, the corners come 0, 2, 1.
    assert_eq!(c[4], [f(0.0), f(1.0), f(0.0)]);
    assert_eq!(c[5], [f(-1.0), f(0.0), f(0.0)]);
}

#[test]
fn copies_of_copies_stop_at_the_limit() {
    // Each subtree is a group of two references to the one below: 2^25
    // copies of the leaf.
    let mut nodes = vec![leaf(0, None)];
    let mut subtrees = vec![0];
    for level in 0..25 {
        let r = nodes.len();
        nodes.push(Node {
            meta: None,
            transform: None,
            kind: NodeKind::Reference { subtree: level },
        });
        nodes.push(Node {
            meta: None,
            transform: None,
            kind: NodeKind::Reference { subtree: level },
        });
        nodes.push(Node {
            meta: None,
            transform: None,
            kind: NodeKind::Group {
                children: vec![r, r + 1],
            },
        });
        subtrees.push(nodes.len() - 1);
    }
    let scene = Scene {
        meshes: vec![triangle()],
        roots: vec![*subtrees.last().unwrap()],
        nodes,
        subtrees,
        ..Scene::default()
    };
    let err = place(&scene).unwrap_err();
    assert!(err.0.contains(&PLACEMENT_LIMIT.to_string()), "{err}");
}

#[test]
fn counts_are_grouped() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1000), "1,000");
    assert_eq!(grouped(1_234_567), "1,234,567");
}
