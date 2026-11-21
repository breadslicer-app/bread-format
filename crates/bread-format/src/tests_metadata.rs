/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::metadata::{EncodeMetadataError, MetadataEntry, decode_metadata, encode_metadata};
use crate::tests_scene::{scene, u32s};

const ID: [u8; 16] = *b"0123456789abcdef";

fn entries() -> Vec<MetadataEntry> {
    vec![
        MetadataEntry::text(None, "title", "Two wolves"),
        MetadataEntry::text(None, "license", "CC-BY-4.0"),
        MetadataEntry::uuid(None, ID),
        MetadataEntry::text(Some(3), "designer", "A. Designer"),
        MetadataEntry::uuid(Some(4), [7; 16]),
        MetadataEntry {
            node: Some(4),
            key: "example.note".into(),
            value: vec![0, 159, 146, 150],
        },
    ]
}

/// A payload of `(subject, key, value)` entries, as stored.
fn payload(raw: &[(u32, &[u8], &[u8])]) -> Vec<u8> {
    let mut out = u32s(&[raw.len() as u32]);
    for (subject, key, value) in raw {
        out.extend(u32s(&[*subject, key.len() as u32]));
        out.extend(*key);
        out.extend(u32s(&[value.len() as u32]));
        out.extend(*value);
    }
    out
}

#[test]
fn entries_come_back_identical() {
    let scene = scene();
    for entries in [vec![], entries()] {
        let payload = encode_metadata(&entries, &scene).unwrap();
        assert_eq!(decode_metadata(&payload, &scene), entries);
    }
}

#[test]
fn the_payload_holds_subject_key_and_value() {
    let entries = [
        MetadataEntry::text(None, "title", "T"),
        MetadataEntry::uuid(Some(2), ID),
    ];
    assert_eq!(
        encode_metadata(&entries, &scene()).unwrap(),
        payload(&[(u32::MAX, b"title", b"T"), (2, b"uuid", &ID)])
    );
}

#[test]
fn values_read_as_text_or_uuid_by_their_key() {
    let [title, _, id, _, _, program] = entries().try_into().unwrap();
    assert_eq!(title.as_text(), Some("Two wolves"));
    assert_eq!(title.as_uuid(), None);
    assert_eq!(id.as_uuid(), Some(ID));
    assert_eq!(id.as_text(), None);
    assert_eq!(program.as_text(), None);
}

#[test]
fn a_writer_writes_only_what_the_rules_allow() {
    let scene = scene();
    let refused = |entry: MetadataEntry| encode_metadata(&[entry], &scene).unwrap_err();
    assert_eq!(
        refused(MetadataEntry::text(Some(9), "title", "x")),
        EncodeMetadataError::NoSuchNode { node: 9 }
    );
    for key in ["", "Title", "9lives", "a b", &"a".repeat(65), "tit\u{e9}"] {
        assert_eq!(
            refused(MetadataEntry::text(None, key, "x")),
            EncodeMetadataError::BadKey { key: key.into() },
            "{key:?}"
        );
    }
    assert_eq!(
        refused(MetadataEntry::text(None, "colour", "red")),
        EncodeMetadataError::ReservedKey {
            key: "colour".into()
        }
    );
    let bad_text = MetadataEntry {
        node: None,
        key: "title".into(),
        value: vec![0xff],
    };
    assert_eq!(
        refused(bad_text),
        EncodeMetadataError::BadValue {
            key: "title".into()
        }
    );
    let short_uuid = MetadataEntry {
        value: vec![1; 15],
        ..MetadataEntry::uuid(None, ID)
    };
    assert_eq!(
        refused(short_uuid),
        EncodeMetadataError::BadValue { key: "uuid".into() }
    );
    let twice = [
        MetadataEntry::text(Some(1), "title", "a"),
        MetadataEntry::text(Some(1), "title", "b"),
    ];
    assert_eq!(
        encode_metadata(&twice, &scene),
        Err(EncodeMetadataError::SameEntry {
            key: "title".into()
        })
    );
    // The same key about two subjects is fine, and so is a long program key.
    let fine = [
        MetadataEntry::text(None, "title", "a"),
        MetadataEntry::text(Some(1), "title", "b"),
        MetadataEntry::text(None, &format!("p.{}", "x".repeat(62)), "c"),
    ];
    assert!(encode_metadata(&fine, &scene).is_ok());
}

#[test]
fn a_reader_leaves_out_entries_that_break_a_rule() {
    let scene = scene();
    let file = u32::MAX;
    let stored = payload(&[
        (file, b"title", b"kept"),
        (file, b"colour", b"red"),         // reserved
        (file, b"Title", b"x"),            // not a key
        (file, b"", b"x"),                 // not a key
        (file, b"designer", b"\xff\xfe"),  // not UTF-8
        (file, b"uuid", &[1; 15]),         // not 16 bytes
        (9, b"title", b"lost"),            // no such node
        (file, b"title", b"second"),       // same subject and key
        (3, b"designer", b"\xff"),         // not UTF-8, so absent ...
        (3, b"designer", b"second too"),   // ... and this is still the second
        (2, b"example.anything", b"\xff"), // a program's own: any bytes
    ]);
    assert_eq!(
        decode_metadata(&stored, &scene),
        [
            MetadataEntry::text(None, "title", "kept"),
            MetadataEntry {
                node: Some(2),
                key: "example.anything".into(),
                value: vec![0xff],
            },
        ]
    );
}

#[test]
fn a_malformed_payload_is_no_metadata_at_all() {
    let scene = scene();
    let stored = encode_metadata(&entries(), &scene).unwrap();
    for cut in 0..stored.len() {
        assert_eq!(decode_metadata(&stored[..cut], &scene), [], "cut at {cut}");
    }
    let mut padded = stored;
    padded.push(0);
    assert_eq!(decode_metadata(&padded, &scene), []);
    assert_eq!(decode_metadata(&u32s(&[u32::MAX]), &scene), []);
}
