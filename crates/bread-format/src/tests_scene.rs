/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::mesh::{Mesh, decode_mesh, encode_mesh};
use crate::meta::Meta;
use crate::scene::NodeKind::{Group, Leaf, Reference, SharedLeaf};
use crate::scene::{
    DecodeSceneError, EncodeSceneError, Node, NodeKind, Scene, Transform, decode_scene,
    encode_scene,
};
use crate::tests_mesh::grid;

fn named(name: &str) -> Option<Meta> {
    Some(Meta {
        name: Some(name.into()),
        comment: None,
    })
}

fn moved(x: f64) -> Option<Transform> {
    Some(Transform {
        linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0 / 3.0]],
        translation: [x, -0.0, 1e-300],
    })
}

fn node(meta: Option<Meta>, transform: Option<Transform>, kind: NodeKind) -> Node {
    Node {
        meta,
        transform,
        kind,
    }
}

fn group(children: &[usize]) -> Node {
    node(
        None,
        None,
        Group {
            children: children.to_vec(),
        },
    )
}

/// Every node kind: a shared mesh resource, a mesh two leaves show, a leaf
/// stored with an empty mesh that keeps only its name (as supports are), a
/// group, and a subtree referenced twice.
pub(crate) fn scene() -> Scene {
    let support = Mesh {
        meta: named("support"),
        ..Mesh::default()
    };
    Scene {
        meta: Some(Meta {
            name: Some("plate".into()),
            comment: Some("two wolves".into()),
        }),
        meshes: vec![grid(3), grid(2), support],
        mesh_resources: vec![0],
        nodes: vec![
            node(named("a"), moved(1.0), SharedLeaf { resource: 0 }),
            node(None, None, Leaf { mesh: 1 }),
            node(named("support"), None, Leaf { mesh: 2 }),
            node(
                named("group"),
                moved(2.0),
                Group {
                    children: vec![1, 0, 2],
                },
            ),
            node(None, moved(3.0), Leaf { mesh: 1 }),
            node(None, None, SharedLeaf { resource: 0 }),
            node(
                named("pair"),
                None,
                Group {
                    children: vec![4, 5],
                },
            ),
            node(named("first"), moved(4.0), Reference { subtree: 0 }),
            node(None, None, Reference { subtree: 0 }),
        ],
        subtrees: vec![6],
        roots: vec![7, 3, 8],
    }
}

pub(crate) fn u32s(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn count(n: usize) -> Vec<u8> {
    u32s(&[u32::try_from(n).unwrap()])
}

/// A payload without metadata, meshes or mesh resources: `records`, then
/// `roots`.
fn raw(records: &[Vec<u8>], roots: &[u32]) -> Vec<u8> {
    let mut payload = vec![0];
    payload.extend(u32s(&[0, 0]));
    payload.extend(count(records.len()));
    payload.extend(records.concat());
    payload.extend(count(roots.len()));
    payload.extend(u32s(roots));
    payload
}

fn group_record(children: &[u32]) -> Vec<u8> {
    [vec![0, 0, 0], count(children.len()), u32s(children)].concat()
}

fn subtree_record(root: u32) -> Vec<u8> {
    [vec![5], u32s(&[root])].concat()
}

fn reference_record(subtree: u32) -> Vec<u8> {
    [vec![4, 0, 0], u32s(&[subtree])].concat()
}

/// A scene of nodes only.
fn graph(nodes: Vec<Node>, subtrees: Vec<usize>, roots: Vec<usize>) -> Scene {
    Scene {
        nodes,
        subtrees,
        roots,
        ..Scene::default()
    }
}

#[test]
fn a_scene_comes_back_identical() {
    let scene = scene();
    let payload = encode_scene(&scene).unwrap();
    let back = decode_scene(&payload, scene.meshes.clone()).unwrap();
    assert_eq!(back, scene);
    assert_eq!(encode_scene(&back).unwrap(), payload);
}

#[test]
fn a_scene_comes_back_through_its_chunks_with_the_emptied_leaf() {
    let scene = scene();
    let payload = encode_scene(&scene).unwrap();
    let meshes = scene
        .meshes
        .iter()
        .map(|mesh| decode_mesh(&encode_mesh(mesh).unwrap()).unwrap())
        .collect();
    let back = decode_scene(&payload, meshes).unwrap();
    assert_eq!(back, scene);
    let Leaf { mesh } = back.nodes[2].kind else {
        panic!("node 2 is a leaf");
    };
    assert!(back.meshes[mesh].vertices.is_empty());
    assert_eq!(back.meshes[mesh].meta, named("support"));
}

#[test]
fn the_encoder_writes_the_layout_of_the_spec() {
    let scene = Scene {
        meta: None,
        meshes: vec![Mesh::default()],
        mesh_resources: vec![0],
        nodes: vec![
            node(None, None, Leaf { mesh: 0 }),
            node(named("g"), moved(2.0), Group { children: vec![0] }),
            node(None, None, Reference { subtree: 0 }),
            node(None, None, SharedLeaf { resource: 0 }),
        ],
        subtrees: vec![1],
        roots: vec![2, 3],
    };
    let mut want = vec![0]; // no metadata
    want.extend(u32s(&[1, 1, 0, 5])); // 1 mesh; 1 mesh resource: mesh 0; 5 records:
    want.extend([1, 0, 0]); // a leaf, no metadata, identity,
    want.extend(u32s(&[0])); // mesh 0
    want.extend([0, 1, 1]); // a group with metadata: a name
    want.extend(u32s(&[1])); // of 1 byte,
    want.extend([b'g', 0, 1]); // "g", no comment, a transform:
    for value in [
        0.0,
        -1.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / 3.0,
        2.0,
        -0.0,
        1e-300,
    ] {
        want.extend(f64::to_le_bytes(value));
    }
    want.extend(u32s(&[1, 0])); // 1 child: node 0
    want.push(5); // subtree 0, right after its root:
    want.extend(u32s(&[1])); // node 1
    want.extend([4, 0, 0]); // a reference, no metadata, identity,
    want.extend(u32s(&[0])); // subtree 0
    want.extend([2, 0, 0]); // a leaf with a shared mesh, no metadata, identity,
    want.extend(u32s(&[0])); // mesh resource 0
    want.extend(u32s(&[2, 2, 3])); // 2 roots: nodes 2 and 3
    assert_eq!(encode_scene(&scene).unwrap(), want);
}

#[test]
fn transforms_come_back_bit_for_bit() {
    let odd = Transform {
        linear: [
            [
                -0.0,
                f64::from_bits(1),
                f64::from_bits(0xfff8_0000_dead_beef),
            ],
            [1.0 / 3.0, f64::MAX, f64::NEG_INFINITY],
            [1e-300, 0.0, 2.0],
        ],
        translation: [-1.0, f64::MIN_POSITIVE, 7.0],
    };
    let identity = Transform {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };
    let mut scene = graph(
        vec![group(&[]), group(&[]), group(&[])],
        vec![],
        vec![0, 1, 2],
    );
    scene.nodes[0].transform = Some(odd);
    scene.nodes[1].transform = Some(identity);
    let back = decode_scene(&encode_scene(&scene).unwrap(), vec![]).unwrap();

    let bits = |t: &Transform| {
        t.linear
            .as_flattened()
            .iter()
            .chain(&t.translation)
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(&back.nodes[0].transform.unwrap()), bits(&odd));
    // An identity matrix stays a matrix, apart from the identity flag.
    assert_eq!(back.nodes[1].transform, Some(identity));
    assert_eq!(back.nodes[2].transform, None);
}

#[test]
fn an_empty_scene_round_trips() {
    let payload = encode_scene(&Scene::default()).unwrap();
    assert_eq!(payload, [vec![0], u32s(&[0, 0, 0, 0])].concat());
    assert_eq!(decode_scene(&payload, vec![]).unwrap(), Scene::default());
}

#[test]
fn a_missing_or_extra_mesh_is_reported() {
    let scene = scene();
    let payload = encode_scene(&scene).unwrap();
    let mut meshes = scene.meshes;
    meshes.push(Mesh::default());
    assert_eq!(
        decode_scene(&payload, meshes.clone()).unwrap_err(),
        DecodeSceneError::MeshCount { named: 3, found: 4 }
    );
    meshes.truncate(2);
    assert_eq!(
        decode_scene(&payload, meshes).unwrap_err(),
        DecodeSceneError::MeshCount { named: 3, found: 2 }
    );
}

#[test]
fn a_cut_or_padded_payload_is_malformed() {
    let scene = scene();
    let payload = encode_scene(&scene).unwrap();
    for cut in 0..payload.len() {
        assert!(
            decode_scene(&payload[..cut], scene.meshes.clone()).is_err(),
            "cut at {cut}"
        );
    }
    let mut padded = payload;
    padded.push(0);
    assert_eq!(
        decode_scene(&padded, scene.meshes).unwrap_err(),
        DecodeSceneError::Malformed
    );
}

#[test]
fn records_name_only_what_is_listed_before_them() {
    assert!(decode_scene(&raw(&[group_record(&[]), group_record(&[0])], &[1]), vec![]).is_ok());
    let cases = [
        ("a group naming itself", raw(&[group_record(&[0])], &[])),
        (
            "a group naming a later node",
            raw(&[group_record(&[1]), group_record(&[])], &[]),
        ),
        (
            "a subtree naming a later node",
            raw(&[subtree_record(0), group_record(&[])], &[]),
        ),
        (
            "a reference naming a later subtree",
            raw(
                &[group_record(&[]), reference_record(0), subtree_record(0)],
                &[],
            ),
        ),
        (
            "a root naming a missing node",
            raw(&[group_record(&[])], &[1]),
        ),
        (
            "a leaf naming a missing mesh",
            raw(&[[vec![1, 0, 0], u32s(&[0])].concat()], &[]),
        ),
        (
            "a leaf naming a missing mesh resource",
            raw(&[[vec![2, 0, 0], u32s(&[0])].concat()], &[]),
        ),
        (
            "a mesh resource naming a missing mesh",
            [vec![0], u32s(&[0, 1, 0, 0, 0])].concat(),
        ),
    ];
    for (case, payload) in cases {
        assert_eq!(
            decode_scene(&payload, vec![]).unwrap_err(),
            DecodeSceneError::Malformed,
            "{case}"
        );
    }
}

#[test]
fn each_node_has_one_place() {
    let cases = [
        (
            "a child twice in one group",
            raw(&[group_record(&[]), group_record(&[0, 0])], &[]),
        ),
        (
            "a child of two groups",
            raw(
                &[group_record(&[]), group_record(&[0]), group_record(&[0])],
                &[],
            ),
        ),
        (
            "a child and a root",
            raw(&[group_record(&[]), group_record(&[0])], &[1, 0]),
        ),
        ("a root twice", raw(&[group_record(&[])], &[0, 0])),
        (
            "a subtree's root and a child",
            raw(
                &[group_record(&[]), subtree_record(0), group_record(&[0])],
                &[],
            ),
        ),
        (
            "a subtree's root and a root",
            raw(&[group_record(&[]), subtree_record(0)], &[0]),
        ),
        (
            "the root of two subtrees",
            raw(
                &[group_record(&[]), subtree_record(0), subtree_record(0)],
                &[],
            ),
        ),
    ];
    for (case, payload) in cases {
        assert_eq!(
            decode_scene(&payload, vec![]).unwrap_err(),
            DecodeSceneError::Malformed,
            "{case}"
        );
    }
}

#[test]
fn other_record_kinds_are_malformed() {
    // Kind 3 was a CSG node in Breadslicer's first files.
    for kind in [3, 6, 255] {
        let record = [vec![kind, 0, 0, 0], u32s(&[0])].concat();
        assert_eq!(
            decode_scene(&raw(&[record], &[]), vec![]).unwrap_err(),
            DecodeSceneError::Malformed,
            "kind {kind}"
        );
    }
}

#[test]
fn a_node_without_a_place_is_kept_at_its_index() {
    let payload = raw(&[group_record(&[]), group_record(&[])], &[1]);
    let scene = decode_scene(&payload, vec![]).unwrap();
    assert_eq!(scene.nodes.len(), 2);
    assert_eq!(scene.roots, [1]);
    assert_eq!(encode_scene(&scene).unwrap(), payload);
}

#[test]
fn subtrees_keep_their_numbers() {
    let payload = raw(
        &[
            group_record(&[]),
            group_record(&[]),
            subtree_record(1),
            subtree_record(0),
            reference_record(0),
            reference_record(1),
        ],
        &[2, 3],
    );
    let scene = decode_scene(&payload, vec![]).unwrap();
    assert_eq!(scene.subtrees, [1, 0]);
    assert_eq!(encode_scene(&scene).unwrap(), payload);
}

#[test]
fn a_damaged_count_cannot_ask_for_huge_memory() {
    let payload = raw(&[group_record(&[])], &[0]);
    assert!(decode_scene(&payload, vec![]).is_ok());
    // Mesh resources, records, the group's children, roots.
    for at in [5, 9, 16, 20] {
        let mut damaged = payload.clone();
        damaged[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode_scene(&damaged, vec![]).unwrap_err(),
            DecodeSceneError::Malformed,
            "count at {at}"
        );
    }
}

#[test]
fn the_encoder_refuses_an_inconsistent_scene() {
    let reference = |subtree| node(None, None, Reference { subtree });
    let cases = [
        (
            "a child listed after its group",
            graph(vec![group(&[1]), group(&[])], vec![], vec![]),
        ),
        (
            "a node with two places",
            graph(vec![group(&[]), group(&[0])], vec![], vec![0]),
        ),
        (
            "a missing mesh",
            graph(vec![node(None, None, Leaf { mesh: 0 })], vec![], vec![]),
        ),
        (
            "a missing mesh resource",
            graph(
                vec![node(None, None, SharedLeaf { resource: 0 })],
                vec![],
                vec![],
            ),
        ),
        (
            "a mesh resource naming a missing mesh",
            Scene {
                mesh_resources: vec![0],
                ..Scene::default()
            },
        ),
        (
            "a reference listed before its subtree's root",
            graph(vec![reference(0), group(&[])], vec![1], vec![]),
        ),
        (
            "a missing subtree",
            graph(vec![reference(0)], vec![], vec![]),
        ),
        (
            "a subtree whose root is missing",
            graph(vec![group(&[])], vec![1], vec![]),
        ),
    ];
    for (case, scene) in cases {
        assert_eq!(
            encode_scene(&scene).unwrap_err(),
            EncodeSceneError::Inconsistent,
            "{case}"
        );
    }
}
