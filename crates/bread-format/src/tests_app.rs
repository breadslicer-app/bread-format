/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, LeafDocument, decode_documents, encode_documents,
};
use crate::tests_scene::{scene, u32s};

/// Supports on `leaf`, as a generator might describe them.
fn document(leaf: usize) -> LeafDocument {
    LeafDocument {
        leaf,
        generator: "df".into(),
        version: 3,
        document: br#"{"roots":[{"tip":{"x":1,"y":2,"z":3}}]}"#.to_vec(),
    }
}

#[test]
fn documents_come_back_with_their_leaves() {
    let scene = scene();
    let documents = vec![
        // The leaf stored with an empty mesh, as supports are.
        document(2),
        // A leaf with a shared mesh, and a generator and version the crate
        // passes on without judging them.
        LeafDocument {
            leaf: 0,
            generator: "someone else".into(),
            version: u32::MAX,
            document: Vec::new(),
        },
        document(4),
    ];
    let payload = encode_documents(&documents, &scene).unwrap();
    assert_eq!(decode_documents(&payload, &scene).unwrap(), documents);
}

#[test]
fn the_encoder_writes_the_envelope_of_the_spec() {
    let document = document(2);
    let want = [
        u32s(&[1, 2, 2]), // 1 entry: leaf 2, a generator name of 2 bytes:
        b"df".to_vec(),
        u32s(&[3, u32::try_from(document.document.len()).unwrap()]), // version 3, the document
        document.document.clone(),
    ]
    .concat();
    assert_eq!(encode_documents(&[document], &scene()).unwrap(), want);
}

#[test]
fn no_documents_round_trip() {
    let payload = encode_documents(&[], &scene()).unwrap();
    assert_eq!(payload, u32s(&[0]));
    assert!(decode_documents(&payload, &scene()).unwrap().is_empty());
}

#[test]
fn documents_must_name_a_leaf() {
    let scene = scene();
    // A group, a reference, and a node the scene does not have.
    for node in [3, 7, 9] {
        assert_eq!(
            encode_documents(&[document(node)], &scene).unwrap_err(),
            EncodeDocumentsError::NotALeaf,
            "node {node}"
        );
        // 1 entry: the node, an empty generator name, version 0, no document.
        let payload = u32s(&[1, u32::try_from(node).unwrap(), 0, 0, 0]);
        assert_eq!(
            decode_documents(&payload, &scene).unwrap_err(),
            DecodeDocumentsError::NotALeaf,
            "node {node}"
        );
    }
}

#[test]
fn a_generator_name_that_is_not_utf8_is_malformed() {
    let payload = [u32s(&[1, 2, 2]), vec![0xff, 0xfe], u32s(&[1, 0])].concat();
    assert_eq!(
        decode_documents(&payload, &scene()).unwrap_err(),
        DecodeDocumentsError::Malformed
    );
}

#[test]
fn a_cut_or_padded_payload_is_malformed() {
    let scene = scene();
    let payload = encode_documents(&[document(2), document(0)], &scene).unwrap();
    for cut in 0..payload.len() {
        assert_eq!(
            decode_documents(&payload[..cut], &scene).unwrap_err(),
            DecodeDocumentsError::Malformed,
            "cut at {cut}"
        );
    }
    let mut padded = payload;
    padded.push(0);
    assert_eq!(
        decode_documents(&padded, &scene).unwrap_err(),
        DecodeDocumentsError::Malformed
    );
}

#[test]
fn a_damaged_count_cannot_ask_for_huge_memory() {
    let scene = scene();
    let payload = encode_documents(&[document(2)], &scene).unwrap();
    // The entry count, the generator name's length, the document's length.
    for at in [0, 8, 18] {
        let mut damaged = payload.clone();
        damaged[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode_documents(&damaged, &scene).unwrap_err(),
            DecodeDocumentsError::Malformed,
            "length at {at}"
        );
    }
}
