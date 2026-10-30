/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::mesh::{DecodeMeshError, EncodeMeshError, Mesh, encode_mesh};
use crate::meta::Meta;
use crate::preview::{
    DecodePreviewsError, EncodePreviewsError, SupportPreview, decode_previews, encode_previews,
};
use crate::tests_mesh::grid;
use crate::tests_scene::{scene, u32s};

/// The supports of the test scene's support leaf, node 2.
pub(crate) fn preview() -> SupportPreview {
    SupportPreview {
        leaf: 2,
        mesh: Mesh {
            meta: Some(Meta {
                name: Some("supports".into()),
                comment: None,
            }),
            ..grid(2)
        },
    }
}

#[test]
fn previews_come_back_identical() {
    let scene = scene();
    for previews in [vec![], vec![preview()]] {
        let payload = encode_previews(&previews, &scene).unwrap();
        assert_eq!(decode_previews(&payload, &scene).unwrap(), previews);
    }
}

#[test]
fn the_payload_holds_the_leaf_and_a_mesh_chunk_payload() {
    let preview = preview();
    let mesh = encode_mesh(&preview.mesh).unwrap();
    let mut expected = u32s(&[1, 2, u32::try_from(mesh.len()).unwrap()]);
    expected.extend_from_slice(&mesh);
    assert_eq!(encode_previews(&[preview], &scene()).unwrap(), expected);
}

#[test]
fn a_preview_must_name_a_leaf_with_an_empty_mesh() {
    let scene = scene();
    for (leaf, encode, decode) in [
        // A group, and a node past the last one.
        (
            3,
            EncodePreviewsError::NotALeaf,
            DecodePreviewsError::NotALeaf,
        ),
        (
            99,
            EncodePreviewsError::NotALeaf,
            DecodePreviewsError::NotALeaf,
        ),
        // A leaf that shows a mesh of its own.
        (
            1,
            EncodePreviewsError::MeshNotEmpty,
            DecodePreviewsError::MeshNotEmpty,
        ),
    ] {
        let previews = [SupportPreview { leaf, ..preview() }];
        assert_eq!(encode_previews(&previews, &scene), Err(encode));
        let payload = encode_previews(&[preview()], &scene).unwrap();
        let mut changed = payload.clone();
        changed[4..8].copy_from_slice(&u32::try_from(leaf).unwrap().to_le_bytes());
        assert_eq!(decode_previews(&changed, &scene), Err(decode));
    }
}

#[test]
fn two_previews_of_one_leaf_are_refused() {
    let scene = scene();
    let previews = [preview(), preview()];
    assert_eq!(
        encode_previews(&previews, &scene),
        Err(EncodePreviewsError::SameLeaf)
    );
    let one = encode_previews(&[preview()], &scene).unwrap();
    let mut two = u32s(&[2]);
    two.extend_from_slice(&one[4..]);
    two.extend_from_slice(&one[4..]);
    assert_eq!(
        decode_previews(&two, &scene),
        Err(DecodePreviewsError::SameLeaf)
    );
}

#[test]
fn a_damaged_payload_is_malformed() {
    let scene = scene();
    let payload = encode_previews(&[preview()], &scene).unwrap();
    let mut padded = payload.clone();
    padded.push(0);
    for broken in [
        &payload[..payload.len() - 1],
        &padded,
        &payload[..3],
        // A count that the rest of the payload cannot hold.
        &u32s(&[u32::MAX])[..],
    ] {
        assert_eq!(
            decode_previews(broken, &scene),
            Err(DecodePreviewsError::Malformed)
        );
    }
}

#[test]
fn a_preview_mesh_is_checked_like_a_mesh_chunk() {
    let scene = scene();
    let mut broken = preview();
    broken.mesh.triangles[0][0] = 99;
    assert_eq!(
        encode_previews(&[broken], &scene),
        Err(EncodePreviewsError::Mesh(EncodeMeshError::VertexOutOfRange))
    );
    // A mesh whose vertex buffer does not start with version 1 of its codec.
    let mut payload = encode_previews(&[preview()], &scene).unwrap();
    let mesh = encode_mesh(&preview().mesh).unwrap();
    let vertex_buffer = payload.len() - mesh.len() + mesh.iter().position(|&b| b == 0xa1).unwrap();
    payload[vertex_buffer] = 0xa0;
    assert_eq!(
        decode_previews(&payload, &scene),
        Err(DecodePreviewsError::Mesh(DecodeMeshError::Malformed))
    );
}
