/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::surface::{
    DecodeSurfaceError, EncodeSurfaceError, SurfaceEntry, decode_surface, encode_surface,
};
use crate::tests_scene::{scene, u32s};

/// Paint of `kind` on the test scene's leaf `leaf`.
pub(crate) fn paint(leaf: usize, kind: u8) -> SurfaceEntry {
    SurfaceEntry {
        leaf,
        kind,
        version: 1,
        document: format!("mask {leaf} {kind}").into_bytes(),
    }
}

#[test]
fn entries_come_back_identical() {
    let scene = scene();
    for entries in [
        vec![],
        vec![paint(0, 0)],
        vec![paint(0, 0), paint(0, 2), paint(4, 1)],
    ] {
        let payload = encode_surface(&entries, &scene).unwrap();
        assert_eq!(decode_surface(&payload, &scene).unwrap(), entries);
    }
}

#[test]
fn the_payload_holds_the_envelope_and_the_document() {
    let entry = paint(1, 2);
    let mut expected = u32s(&[1, 1]);
    expected.push(2);
    expected.extend(u32s(&[1, entry.document.len() as u32]));
    expected.extend(&entry.document);
    assert_eq!(encode_surface(&[entry], &scene()).unwrap(), expected);
}

#[test]
fn reserved_kinds_and_newer_documents_are_carried() {
    let scene = scene();
    let entries = vec![
        SurfaceEntry {
            kind: 200,
            ..paint(0, 0)
        },
        SurfaceEntry {
            version: 99,
            document: b"not a mask at all".to_vec(),
            ..paint(1, 0)
        },
    ];
    let payload = encode_surface(&entries, &scene).unwrap();
    assert_eq!(decode_surface(&payload, &scene).unwrap(), entries);
}

#[test]
fn an_entry_must_name_a_leaf() {
    let scene = scene();
    // Node 3 is a group, node 7 a reference, and there is no node 9.
    for leaf in [3, 7, 9] {
        let entry = paint(leaf, 0);
        assert_eq!(
            encode_surface(std::slice::from_ref(&entry), &scene),
            Err(EncodeSurfaceError::NotALeaf),
            "node {leaf}"
        );
        let payload = encode_surface(&[paint(0, 0)], &scene)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(i, byte)| if i == 4 { leaf as u8 } else { byte })
            .collect::<Vec<_>>();
        assert_eq!(
            decode_surface(&payload, &scene),
            Err(DecodeSurfaceError::NotALeaf),
            "node {leaf}"
        );
    }
}

#[test]
fn two_entries_may_not_share_a_leaf_and_a_kind() {
    let scene = scene();
    let twice = [paint(0, 1), paint(0, 1)];
    assert_eq!(
        encode_surface(&twice, &scene),
        Err(EncodeSurfaceError::SameEntry)
    );
    let mut payload = encode_surface(&twice[..1], &scene).unwrap();
    let entry = payload[4..].to_vec();
    payload[..4].copy_from_slice(&2u32.to_le_bytes());
    payload.extend(entry);
    assert_eq!(
        decode_surface(&payload, &scene),
        Err(DecodeSurfaceError::SameEntry)
    );
}

#[test]
fn a_cut_short_or_padded_payload_is_malformed() {
    let scene = scene();
    let payload = encode_surface(&[paint(0, 0), paint(1, 1)], &scene).unwrap();
    for cut in 0..payload.len() {
        assert_eq!(
            decode_surface(&payload[..cut], &scene),
            Err(DecodeSurfaceError::Malformed),
            "cut at {cut}"
        );
    }
    let mut padded = payload;
    padded.push(0);
    assert_eq!(
        decode_surface(&padded, &scene),
        Err(DecodeSurfaceError::Malformed)
    );
}

#[test]
fn a_huge_count_is_malformed_before_any_allocation() {
    let payload = u32s(&[u32::MAX]);
    assert_eq!(
        decode_surface(&payload, &scene()),
        Err(DecodeSurfaceError::Malformed)
    );
}
