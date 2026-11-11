/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::mesh::{DecodeMeshError, EncodeMeshError, Mesh};
use crate::mesh_v2::{
    Connectivity, MeshCoding, VertexCoding, decode_mesh_v2, encode_mesh_v2_with, in_first_use_order,
};
use crate::meta::Meta;
use crate::tests_mesh::grid;

const CONNECTIVITIES: [Connectivity; 2] = [Connectivity::InOrder, Connectivity::Reordered];
const VERTEX_CODINGS: [VertexCoding; 3] = [
    VertexCoding::Meshopt,
    VertexCoding::Rans,
    VertexCoding::Planes,
];

fn codings() -> impl Iterator<Item = MeshCoding> {
    CONNECTIVITIES.into_iter().flat_map(|connectivity| {
        VERTEX_CODINGS.into_iter().map(move |vertices| MeshCoding {
            connectivity,
            vertices,
        })
    })
}

/// `mesh` with its vertices renumbered by first use, and unused ones dropped,
/// as a mesh welded from an STL is.
pub(crate) fn welded(mesh: &Mesh) -> Mesh {
    let mut renumber = vec![u32::MAX; mesh.vertices.len()];
    let mut vertices = Vec::new();
    let triangles = mesh
        .triangles
        .iter()
        .map(|t| {
            t.map(|v| {
                if renumber[v as usize] == u32::MAX {
                    renumber[v as usize] = vertices.len() as u32;
                    vertices.push(mesh.vertices[v as usize]);
                }
                renumber[v as usize]
            })
        })
        .collect();
    Mesh {
        meta: mesh.meta.clone(),
        vertices,
        triangles,
    }
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

fn round_trip(mesh: &Mesh, coding: MeshCoding) {
    let payload = encode_mesh_v2_with(mesh, coding).unwrap();
    let back = decode_mesh_v2(&payload).unwrap();
    assert!(same(&back, mesh), "{coding:?}");
    assert_eq!(
        encode_mesh_v2_with(&back, coding).unwrap(),
        payload,
        "{coding:?}"
    );
}

/// A grid with odd values: NaN with a payload, both infinities, -0.0, a
/// subnormal and the largest float.
fn odd_grid() -> Mesh {
    let mut mesh = welded(&grid(6));
    mesh.vertices[1] = [f32::from_bits(0x7fc0_1234), f32::INFINITY, -0.0];
    mesh.vertices[2] = [f32::NEG_INFINITY, f32::from_bits(1), f32::MAX];
    mesh.meta = Some(Meta {
        name: Some("odd".into()),
        comment: None,
    });
    mesh
}

#[test]
fn every_coding_comes_back_bit_for_bit() {
    for mesh in [welded(&grid(8)), odd_grid()] {
        assert!(in_first_use_order(&mesh));
        for coding in codings() {
            round_trip(&mesh, coding);
        }
    }
}

#[test]
fn connectivity_a_keeps_any_vertex_order_and_unused_vertices() {
    // grid() lists its faces scrambled, so its vertices are not in first-use
    // order; add a vertex no triangle uses.
    let mut mesh = grid(5);
    mesh.vertices.push([9.0, 9.0, 9.0]);
    assert!(!in_first_use_order(&mesh));
    for vertices in VERTEX_CODINGS {
        round_trip(
            &mesh,
            MeshCoding {
                connectivity: Connectivity::InOrder,
                vertices,
            },
        );
    }
}

#[test]
fn connectivity_b_needs_first_use_order() {
    let coding = MeshCoding {
        connectivity: Connectivity::Reordered,
        vertices: VertexCoding::Meshopt,
    };
    assert_eq!(
        encode_mesh_v2_with(&grid(3), coding),
        Err(EncodeMeshError::Unsupported)
    );
}

#[test]
fn rans_needs_at_most_three_vertices_per_triangle() {
    let mesh = Mesh {
        meta: None,
        vertices: vec![[0.0; 3]; 4],
        triangles: vec![[0, 1, 2]],
    };
    let coding = MeshCoding {
        connectivity: Connectivity::InOrder,
        vertices: VertexCoding::Rans,
    };
    assert_eq!(
        encode_mesh_v2_with(&mesh, coding),
        Err(EncodeMeshError::Unsupported)
    );
}

#[test]
fn degenerate_and_shared_edges_come_back() {
    // Repeated corners, and one edge shared by five triangles.
    let mesh = Mesh {
        meta: None,
        vertices: (0..8)
            .map(|i| [i as f32, (i * i) as f32 * 0.5, 1.0 / (i + 1) as f32])
            .collect(),
        triangles: vec![
            [0, 1, 2],
            [1, 0, 3],
            [0, 1, 4],
            [1, 0, 5],
            [0, 1, 6],
            [2, 2, 7],
            [7, 7, 7],
            [3, 4, 3],
        ],
    };
    assert!(in_first_use_order(&mesh));
    for coding in codings() {
        round_trip(&mesh, coding);
    }
}

#[test]
fn an_empty_mesh_comes_back() {
    let mesh = Mesh::default();
    for vertices in VERTEX_CODINGS {
        round_trip(
            &mesh,
            MeshCoding {
                connectivity: Connectivity::InOrder,
                vertices,
            },
        );
    }
}

#[test]
fn every_cut_and_padding_is_refused() {
    let mesh = welded(&grid(3));
    for coding in codings() {
        let payload = encode_mesh_v2_with(&mesh, coding).unwrap();
        for cut in 0..payload.len() {
            assert!(
                decode_mesh_v2(&payload[..cut]).is_err(),
                "{coding:?} cut at {cut}"
            );
        }
        let mut padded = payload;
        padded.push(0);
        assert_eq!(
            decode_mesh_v2(&padded),
            Err(DecodeMeshError::Malformed),
            "{coding:?}"
        );
    }
}

#[test]
fn every_flipped_byte_is_refused_or_decodes_without_panicking() {
    let mesh = welded(&grid(3));
    for coding in codings() {
        let payload = encode_mesh_v2_with(&mesh, coding).unwrap();
        for i in 0..payload.len() {
            for flip in [0x01, 0x80, 0xff] {
                let mut damaged = payload.clone();
                damaged[i] ^= flip;
                let _ = decode_mesh_v2(&damaged);
            }
        }
    }
}

/// The offset of the coding bytes in a payload without metadata.
const CODINGS_AT: usize = 1 + 4 + 4;

/// A small welded grid without metadata, so the counts sit at bytes 1 and 5.
fn plain() -> Mesh {
    Mesh {
        meta: None,
        ..welded(&grid(2))
    }
}

#[test]
fn unknown_coding_bytes_are_refused() {
    let mesh = plain();
    let payload = encode_mesh_v2_with(
        &mesh,
        MeshCoding {
            connectivity: Connectivity::InOrder,
            vertices: VertexCoding::Meshopt,
        },
    )
    .unwrap();
    for (at, value) in [(CODINGS_AT, 2), (CODINGS_AT + 1, 3), (CODINGS_AT + 1, 255)] {
        let mut changed = payload.clone();
        changed[at] = value;
        assert_eq!(
            decode_mesh_v2(&changed),
            Err(DecodeMeshError::Malformed),
            "byte {at} = {value}"
        );
    }
}

#[test]
fn huge_counts_are_refused_before_any_allocation() {
    let mesh = plain();
    for coding in codings() {
        let payload = encode_mesh_v2_with(&mesh, coding).unwrap();
        for at in [1, 5] {
            let mut changed = payload.clone();
            changed[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(decode_mesh_v2(&changed).is_err(), "{coding:?} at {at}");
        }
    }
}

#[test]
fn rans_with_more_than_three_vertices_per_triangle_is_refused() {
    let mesh = plain();
    let mut payload = encode_mesh_v2_with(
        &mesh,
        MeshCoding {
            connectivity: Connectivity::InOrder,
            vertices: VertexCoding::Rans,
        },
    )
    .unwrap();
    let n = 3 * mesh.triangles.len() as u32 + 1;
    payload[1..5].copy_from_slice(&n.to_le_bytes());
    assert_eq!(decode_mesh_v2(&payload), Err(DecodeMeshError::Malformed));
}

#[test]
fn a_vertex_count_the_planes_do_not_hold_is_refused() {
    let mesh = plain();
    let mut payload = encode_mesh_v2_with(
        &mesh,
        MeshCoding {
            connectivity: Connectivity::InOrder,
            vertices: VertexCoding::Planes,
        },
    )
    .unwrap();
    // One vertex fewer than the 12 planes hold.
    let n = mesh.vertices.len() as u32 - 1;
    payload[1..5].copy_from_slice(&n.to_le_bytes());
    assert!(decode_mesh_v2(&payload).is_err());
}
