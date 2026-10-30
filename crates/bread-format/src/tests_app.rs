/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, HollowEntry, SupportEntry, decode_hollows,
    decode_supports, encode_hollows, encode_supports,
};
use crate::tests_scene::{scene, u32s};

const DOCUMENT: &[u8] = br#"{"roots":[{"tip":{"x":1,"y":2,"z":3}}]}"#;

/// Supports for `model` shown by `support`, as a generator might describe
/// them.
fn supports(model: usize, support: usize) -> SupportEntry {
    SupportEntry {
        model,
        support,
        generator: "df".into(),
        version: 3,
        document: DOCUMENT.to_vec(),
    }
}

/// A hollow of `model` made from the mesh `original`.
fn hollow(model: usize, original: usize) -> HollowEntry {
    HollowEntry {
        model,
        original,
        generator: "df".into(),
        version: 3,
        document: DOCUMENT.to_vec(),
    }
}

/// A payload of entries naming `(model, part)`, each with an empty generator
/// name, version 0 and an empty document.
fn payload(entries: &[(u32, u32)]) -> Vec<u8> {
    let mut values = vec![u32::try_from(entries.len()).unwrap()];
    for &(model, part) in entries {
        values.extend([model, part, 0, 0, 0]);
    }
    u32s(&values)
}

fn supports_from(entries: &[(usize, usize)]) -> Vec<SupportEntry> {
    entries.iter().map(|&(m, s)| supports(m, s)).collect()
}

fn hollows_from(entries: &[(usize, usize)]) -> Vec<HollowEntry> {
    entries.iter().map(|&(m, o)| hollow(m, o)).collect()
}

fn as_u32(entries: &[(usize, usize)]) -> Vec<(u32, u32)> {
    entries
        .iter()
        .map(|&(a, b)| (u32::try_from(a).unwrap(), u32::try_from(b).unwrap()))
        .collect()
}

/// Checks that supports naming `entries` are refused both ways.
fn supports_refused(
    entries: &[(usize, usize)],
    encode: EncodeDocumentsError,
    decode: DecodeDocumentsError,
) {
    let scene = scene();
    assert_eq!(
        encode_supports(&supports_from(entries), &scene).unwrap_err(),
        encode,
        "encode {entries:?}"
    );
    let decoded = decode_supports(&payload(&as_u32(entries)), &scene).unwrap_err();
    assert_eq!(decoded, decode, "decode {entries:?}");
}

/// Checks that hollows naming `entries` are refused both ways.
fn hollows_refused(
    entries: &[(usize, usize)],
    encode: EncodeDocumentsError,
    decode: DecodeDocumentsError,
) {
    let scene = scene();
    assert_eq!(
        encode_hollows(&hollows_from(entries), &scene).unwrap_err(),
        encode,
        "encode {entries:?}"
    );
    let decoded = decode_hollows(&payload(&as_u32(entries)), &scene).unwrap_err();
    assert_eq!(decoded, decode, "decode {entries:?}");
}

#[test]
fn supports_come_back_with_their_model_and_leaf() {
    let scene = scene();
    let entries = vec![
        // The support leaf stored with an empty mesh.
        supports(1, 2),
        // A model and a support leaf with a shared mesh, and a generator and
        // version the crate passes on without judging them.
        SupportEntry {
            model: 0,
            support: 5,
            generator: "someone else".into(),
            version: u32::MAX,
            document: Vec::new(),
        },
    ];
    let payload = encode_supports(&entries, &scene).unwrap();
    assert_eq!(decode_supports(&payload, &scene).unwrap(), entries);
}

#[test]
fn hollows_come_back_with_their_model_and_original() {
    let scene = scene();
    // The original of the first is shown by other leaves too, as a solid
    // copy of the model may be.
    let entries = vec![hollow(4, 0), hollow(1, 2)];
    let payload = encode_hollows(&entries, &scene).unwrap();
    assert_eq!(decode_hollows(&payload, &scene).unwrap(), entries);
}

#[test]
fn the_encoders_write_the_envelope_of_the_spec() {
    let length = u32::try_from(DOCUMENT.len()).unwrap();
    // 1 entry: the model, the part, a generator name of 2 bytes, version 3,
    // the document.
    let want = |model, part| {
        [
            u32s(&[1, model, part, 2]),
            b"df".to_vec(),
            u32s(&[3, length]),
            DOCUMENT.to_vec(),
        ]
        .concat()
    };
    let scene = scene();
    assert_eq!(
        encode_supports(&[supports(1, 2)], &scene).unwrap(),
        want(1, 2)
    );
    assert_eq!(encode_hollows(&[hollow(4, 0)], &scene).unwrap(), want(4, 0));
}

#[test]
fn no_entries_round_trip() {
    let scene = scene();
    let payload = encode_supports(&[], &scene).unwrap();
    assert_eq!(payload, u32s(&[0]));
    assert!(decode_supports(&payload, &scene).unwrap().is_empty());
    assert_eq!(encode_hollows(&[], &scene).unwrap(), payload);
    assert!(decode_hollows(&payload, &scene).unwrap().is_empty());
}

#[test]
fn models_and_support_leaves_must_be_leaves() {
    // A group, a reference, and a node the scene does not have.
    for node in [3, 7, 9] {
        supports_refused(
            &[(node, 2)],
            EncodeDocumentsError::NotALeaf,
            DecodeDocumentsError::NotALeaf,
        );
        supports_refused(
            &[(1, node)],
            EncodeDocumentsError::NotALeaf,
            DecodeDocumentsError::NotALeaf,
        );
        hollows_refused(
            &[(node, 0)],
            EncodeDocumentsError::NotALeaf,
            DecodeDocumentsError::NotALeaf,
        );
    }
}

#[test]
fn an_original_must_be_a_mesh_of_the_file() {
    hollows_refused(
        &[(4, 3)],
        EncodeDocumentsError::NoSuchMesh,
        DecodeDocumentsError::NoSuchMesh,
    );
}

#[test]
fn a_model_has_one_entry_per_chunk() {
    supports_refused(
        &[(1, 2), (1, 5)],
        EncodeDocumentsError::SameModel,
        DecodeDocumentsError::SameModel,
    );
    hollows_refused(
        &[(4, 0), (4, 1)],
        EncodeDocumentsError::SameModel,
        DecodeDocumentsError::SameModel,
    );
}

#[test]
fn a_support_leaf_belongs_to_one_model_and_is_no_model() {
    // Two models with one support leaf.
    supports_refused(
        &[(1, 2), (4, 2)],
        EncodeDocumentsError::SharedSupport,
        DecodeDocumentsError::SharedSupport,
    );
    // A support leaf that is the model of another entry, before or after it.
    supports_refused(
        &[(1, 2), (2, 5)],
        EncodeDocumentsError::SharedSupport,
        DecodeDocumentsError::SharedSupport,
    );
    supports_refused(
        &[(2, 5), (1, 2)],
        EncodeDocumentsError::SharedSupport,
        DecodeDocumentsError::SharedSupport,
    );
    // A model that supports itself.
    supports_refused(
        &[(1, 1)],
        EncodeDocumentsError::SharedSupport,
        DecodeDocumentsError::SharedSupport,
    );
}

#[test]
fn a_generator_name_that_is_not_utf8_is_malformed() {
    let payload = [u32s(&[1, 1, 2, 2]), vec![0xff, 0xfe], u32s(&[1, 0])].concat();
    let scene = scene();
    assert_eq!(
        decode_supports(&payload, &scene).unwrap_err(),
        DecodeDocumentsError::Malformed
    );
    assert_eq!(
        decode_hollows(&payload, &scene).unwrap_err(),
        DecodeDocumentsError::Malformed
    );
}

#[test]
fn a_cut_or_padded_payload_is_malformed() {
    let scene = scene();
    let payloads = [
        encode_supports(&[supports(1, 2), supports(0, 5)], &scene).unwrap(),
        encode_hollows(&[hollow(4, 0), hollow(1, 2)], &scene).unwrap(),
    ];
    for payload in payloads {
        for cut in 0..payload.len() {
            let cut_short = &payload[..cut];
            assert_eq!(
                decode_supports(cut_short, &scene).unwrap_err(),
                DecodeDocumentsError::Malformed,
                "cut at {cut}"
            );
            assert_eq!(
                decode_hollows(cut_short, &scene).unwrap_err(),
                DecodeDocumentsError::Malformed,
                "cut at {cut}"
            );
        }
        let mut padded = payload;
        padded.push(0);
        assert_eq!(
            decode_supports(&padded, &scene).unwrap_err(),
            DecodeDocumentsError::Malformed
        );
        assert_eq!(
            decode_hollows(&padded, &scene).unwrap_err(),
            DecodeDocumentsError::Malformed
        );
    }
}

#[test]
fn a_damaged_count_cannot_ask_for_huge_memory() {
    let scene = scene();
    let payload = encode_supports(&[supports(1, 2)], &scene).unwrap();
    // The entry count, the generator name's length, the document's length.
    for at in [0, 12, 22] {
        let mut damaged = payload.clone();
        damaged[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode_supports(&damaged, &scene).unwrap_err(),
            DecodeDocumentsError::Malformed,
            "length at {at}"
        );
        assert_eq!(
            decode_hollows(&damaged, &scene).unwrap_err(),
            DecodeDocumentsError::Malformed,
            "length at {at}"
        );
    }
}

#[test]
fn an_entry_takes_at_least_20_bytes() {
    // A count of 1 with 19 bytes after it.
    let payload = [u32s(&[1]), vec![0; 19]].concat();
    assert_eq!(
        decode_supports(&payload, &scene()).unwrap_err(),
        DecodeDocumentsError::Malformed
    );
}
