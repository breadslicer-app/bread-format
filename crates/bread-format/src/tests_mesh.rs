/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::mesh::{DecodeMeshError, EncodeMeshError, Mesh, MeshMeta, decode_mesh, encode_mesh};

/// A bumpy grid with its triangles shuffled and started at varied corners,
/// so the codec has triangles to rotate, and with awkward coordinates.
pub(crate) fn grid(side: u32) -> Mesh {
    let mut vertices = Vec::new();
    for y in 0..=side {
        for x in 0..=side {
            let (fx, fy) = (x as f32 * 0.1 + 1.0 / 3.0, y as f32 * 0.1);
            vertices.push([fx, fy, (fx * 7.3).sin() * fy]);
        }
    }
    vertices[0] = [-0.0, f32::MIN_POSITIVE / 4.0, f32::MAX];
    let at = |x: u32, y: u32| y * (side + 1) + x;
    let mut faces = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let [a, b, c, d] = [at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)];
            faces.push([a, b, c]);
            faces.push([c, d, a]);
        }
    }
    // A fixed scramble of face order and first corners.
    let n = faces.len();
    let triangles = (0..n)
        .map(|i| {
            let mut face = faces[(i * 7919) % n];
            face.rotate_left(i % 3);
            face
        })
        .collect();
    Mesh {
        meta: Some(MeshMeta {
            name: Some("grid ñ".into()),
            comment: None,
        }),
        vertices,
        triangles,
    }
}

fn assert_identical(back: &Mesh, mesh: &Mesh) {
    let bits = |m: &Mesh| {
        m.vertices
            .iter()
            .map(|v| v.map(f32::to_bits))
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(back), bits(mesh));
    assert_eq!(back.triangles, mesh.triangles);
    assert_eq!(back.meta, mesh.meta);
}

fn round_trip(mesh: &Mesh) -> Mesh {
    decode_mesh(&encode_mesh(mesh).unwrap()).unwrap()
}

/// A payload put together by hand from encoded buffers, with no metadata.
fn payload(
    vertex_count: u32,
    vertices: &[u8],
    triangle_count: u32,
    indices: &[u8],
    rotations: &[(u32, u8)],
) -> Vec<u8> {
    let mut out = vec![0];
    for (count, buffer) in [(vertex_count, vertices), (triangle_count, indices)] {
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&(buffer.len() as u32).to_le_bytes());
        out.extend_from_slice(buffer);
    }
    out.extend_from_slice(&(rotations.len() as u32).to_le_bytes());
    for (gap, corner) in rotations {
        out.extend_from_slice(&gap.to_le_bytes());
        out.push(*corner);
    }
    out
}

/// Where the vertex and index buffers start in a payload without metadata.
fn buffer_starts(payload: &[u8]) -> (usize, usize) {
    let vertex_length = u32::from_le_bytes(payload[5..9].try_into().unwrap()) as usize;
    let vertices = 9;
    (vertices, vertices + vertex_length + 8)
}

#[test]
fn a_mesh_comes_back_identical_with_its_first_corners() {
    let mesh = grid(40);
    // The test only means something if the codec rotates triangles.
    let indices: Vec<u32> = mesh.triangles.iter().flatten().copied().collect();
    let stored: Vec<u32> = meshopt::decode_index_buffer(
        &meshopt::encode_index_buffer(&indices, mesh.vertices.len()).unwrap(),
        indices.len(),
    )
    .unwrap();
    assert_ne!(stored, indices, "the codec rotates some triangles");

    assert_identical(&round_trip(&mesh), &mesh);
}

#[test]
fn metadata_and_its_absence_come_back() {
    let mut mesh = grid(2);
    mesh.meta = Some(MeshMeta {
        name: None,
        comment: Some("solid exported".into()),
    });
    assert_identical(&round_trip(&mesh), &mesh);
    mesh.meta = Some(MeshMeta::default());
    assert_identical(&round_trip(&mesh), &mesh);
    mesh.meta = None;
    assert_identical(&round_trip(&mesh), &mesh);
}

#[test]
fn an_empty_mesh_round_trips() {
    let mesh = Mesh::default();
    assert_identical(&round_trip(&mesh), &mesh);
}

#[test]
fn the_encoder_writes_the_pinned_codec_versions() {
    let mut mesh = grid(10);
    mesh.meta = None;
    let payload = encode_mesh(&mesh).unwrap();
    let (vertices, indices) = buffer_starts(&payload);
    assert_eq!(payload[vertices], 0xa1, "attribute codec version 1");
    assert_eq!(payload[indices], 0xe1, "triangle codec version 1");
}

#[test]
fn other_codec_versions_are_malformed() {
    let mut mesh = grid(3);
    mesh.meta = None;
    let payload = encode_mesh(&mesh).unwrap();
    let (vertices, indices) = buffer_starts(&payload);
    for (at, other) in [(vertices, 0xa0), (vertices, 0xa2), (indices, 0xe0)] {
        let mut changed = payload.clone();
        changed[at] = other;
        assert_eq!(
            decode_mesh(&changed).unwrap_err(),
            DecodeMeshError::Malformed,
            "first byte 0x{other:02x}"
        );
    }
}

#[test]
fn identical_vertices_round_trip() {
    // The vertex codec spends little more than its 3-byte block headers on
    // repeated vertices: about 1000 vertex bytes per byte, well past the 256
    // an earlier bound allowed.
    let mesh = Mesh {
        meta: None,
        vertices: vec![[1.5, -2.25, 3.0]; 100_000],
        triangles: vec![[0, 1, 2]; 10],
    };
    let payload = encode_mesh(&mesh).unwrap();
    let vertex_length = u32::from_le_bytes(payload[5..9].try_into().unwrap()) as usize;
    assert!(vertex_length < 100_000 * 12 / 256, "{vertex_length} bytes");
    assert_identical(&decode_mesh(&payload).unwrap(), &mesh);
}

#[test]
fn a_cut_or_padded_payload_is_malformed() {
    let payload = encode_mesh(&grid(5)).unwrap();
    for cut in [0, 1, 10, payload.len() / 2, payload.len() - 1] {
        assert_eq!(
            decode_mesh(&payload[..cut]).unwrap_err(),
            DecodeMeshError::Malformed,
            "cut at {cut}"
        );
    }
    let mut padded = payload;
    padded.push(0);
    assert_eq!(
        decode_mesh(&padded).unwrap_err(),
        DecodeMeshError::Malformed
    );
}

#[test]
fn a_damaged_count_cannot_ask_for_huge_memory() {
    let empty = encode_mesh(&Mesh::default()).unwrap();
    // The vertex count follows the one-byte "no metadata" flag.
    let mut payload = empty.clone();
    payload[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_mesh(&payload).unwrap_err(),
        DecodeMeshError::Malformed
    );
    // So does the triangle count, after the 25-byte empty vertex buffer.
    let (_, indices) = buffer_starts(&empty);
    let mut payload = empty;
    payload[indices - 8..indices - 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_mesh(&payload).unwrap_err(),
        DecodeMeshError::Malformed
    );
}

#[test]
fn rotations_must_name_each_triangle_once_and_a_real_corner() {
    let vertices: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    ];
    let encoded_vertices = meshopt::encode_vertex_buffer_level(&vertices, 2, Some(1)).unwrap();
    let indices = [0, 1, 2, 2, 1, 3];
    let encoded_indices = meshopt::encode_index_buffer(&indices, 4).unwrap();
    let with = |rotations: &[(u32, u8)]| {
        decode_mesh(&payload(
            4,
            &encoded_vertices,
            2,
            &encoded_indices,
            rotations,
        ))
    };
    // A first gap of 0 names the first triangle.
    assert!(with(&[(0, 1)]).is_ok());
    assert!(with(&[(0, 1), (1, 2)]).is_ok());
    for bad in [
        &[(0, 1), (0, 1)][..],
        &[(2, 1)],
        &[(1, 1), (1, 1)],
        &[(0, 0)],
        &[(0, 3)],
    ] {
        assert_eq!(
            with(bad).unwrap_err(),
            DecodeMeshError::Malformed,
            "{bad:?}"
        );
    }
}

#[test]
fn a_triangle_naming_a_missing_vertex_is_refused() {
    let mesh = Mesh {
        meta: None,
        vertices: vec![[0.0; 3]; 3],
        triangles: vec![[0, 1, 3]],
    };
    assert_eq!(
        encode_mesh(&mesh).unwrap_err(),
        EncodeMeshError::VertexOutOfRange
    );

    let encoded_vertices = meshopt::encode_vertex_buffer_level(&mesh.vertices, 2, Some(1)).unwrap();
    let encoded_indices = meshopt::encode_index_buffer(&[0, 1, 3], 4).unwrap();
    assert_eq!(
        decode_mesh(&payload(3, &encoded_vertices, 1, &encoded_indices, &[])).unwrap_err(),
        DecodeMeshError::VertexOutOfRange
    );
}
