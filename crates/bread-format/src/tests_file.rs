/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, LeafDocument, SETTINGS_TAG,
    SUPPORTS_TAG,
};
use crate::container::{BreadReader, BreadWriter, ChunkTag};
use crate::file::{BreadFile, ReadFileError, WriteFileError, read_file, write_file};
use crate::mesh::{DecodeMeshError, MESH_TAG};
use crate::scene::{DecodeSceneError, SCENE_TAG};
use crate::tests_scene::scene;

fn document(leaf: usize, text: &str) -> LeafDocument {
    LeafDocument {
        leaf,
        generator: "df".into(),
        version: 1,
        document: text.as_bytes().to_vec(),
    }
}

/// The test scene with settings, supports on its leaf with an empty mesh,
/// and a hollow.
fn file() -> BreadFile {
    BreadFile {
        scene: scene(),
        settings: Some(b"version = 1\nprinter = \"example\"\n".to_vec()),
        supports: vec![document(2, "tips, trunks and rafts")],
        hollows: vec![document(4, "original mesh, settings and drains")],
    }
}

fn write(file: &BreadFile) -> Vec<u8> {
    write_file(file, Vec::new()).unwrap()
}

fn tags(bytes: &[u8]) -> Vec<ChunkTag> {
    let mut reader = BreadReader::new(bytes).unwrap();
    let mut tags = Vec::new();
    while let Some(chunk) = reader.next_chunk().unwrap() {
        tags.push(chunk.tag);
    }
    tags
}

/// The file's chunks, rewritten through `edit`.
fn rewrite(
    bytes: &[u8],
    edit: impl Fn(ChunkTag, u16, Vec<u8>) -> Vec<(ChunkTag, u16, Vec<u8>)>,
) -> Vec<u8> {
    let mut reader = BreadReader::new(bytes).unwrap();
    let mut writer = BreadWriter::new(Vec::new()).unwrap();
    while let Some(chunk) = reader.next_chunk().unwrap() {
        for (tag, version, payload) in edit(chunk.tag, chunk.version, chunk.payload) {
            writer.chunk(tag, version, &payload).unwrap();
        }
    }
    writer.finish().unwrap()
}

/// The file without the chunks of `tag`.
fn without(bytes: &[u8], tag: ChunkTag) -> Vec<u8> {
    rewrite(bytes, |t, version, payload| {
        if t == tag {
            vec![]
        } else {
            vec![(t, version, payload)]
        }
    })
}

#[test]
fn a_file_comes_back_identical() {
    let file = file();
    let bytes = write(&file);
    let back = read_file(bytes.as_slice()).unwrap();
    assert_eq!(back, file);
    assert_eq!(write(&back), bytes);
}

#[test]
fn chunks_come_in_order() {
    assert_eq!(
        tags(&write(&file())),
        [
            SCENE_TAG,
            SETTINGS_TAG,
            SUPPORTS_TAG,
            HOLLOWS_TAG,
            MESH_TAG,
            MESH_TAG,
            MESH_TAG
        ]
    );
}

#[test]
fn a_file_without_application_data_has_no_application_chunks() {
    let file = BreadFile {
        scene: scene(),
        ..BreadFile::default()
    };
    let bytes = write(&file);
    assert_eq!(tags(&bytes), [SCENE_TAG, MESH_TAG, MESH_TAG, MESH_TAG]);
    assert_eq!(read_file(bytes.as_slice()).unwrap(), file);
}

#[test]
fn unknown_chunks_are_skipped() {
    let bytes = write(&file());
    let extended = rewrite(&bytes, |tag, version, payload| {
        vec![
            (ChunkTag(*b"NEW1"), 9, b"from a newer build".to_vec()),
            (ChunkTag(*b"priv"), 1, b"a program's own data".to_vec()),
            (tag, version, payload),
        ]
    });
    assert_eq!(read_file(extended.as_slice()).unwrap(), file());
}

#[test]
fn a_newer_version_or_version_0_is_refused() {
    let bytes = write(&file());
    for tag in [SCENE_TAG, SETTINGS_TAG, SUPPORTS_TAG, HOLLOWS_TAG, MESH_TAG] {
        for bad in [0, 2] {
            let changed = rewrite(&bytes, |t, version, payload| {
                vec![(t, if t == tag { bad } else { version }, payload)]
            });
            assert!(
                matches!(
                    read_file(changed.as_slice()),
                    Err(ReadFileError::ChunkVersion { tag: t, version }) if t == tag && version == bad
                ),
                "{tag} at version {bad}"
            );
        }
    }
}

#[test]
fn a_file_without_a_scene_is_refused() {
    let headless = without(&write(&file()), SCENE_TAG);
    assert!(matches!(
        read_file(headless.as_slice()),
        Err(ReadFileError::ChunkCount { tag, count: 0 }) if tag == SCENE_TAG
    ));
}

#[test]
fn two_of_a_chunk_that_comes_once_are_refused() {
    let bytes = write(&file());
    for tag in [SCENE_TAG, SETTINGS_TAG, SUPPORTS_TAG, HOLLOWS_TAG] {
        let doubled = rewrite(&bytes, |t, version, payload| {
            vec![(t, version, payload); if t == tag { 2 } else { 1 }]
        });
        assert!(
            matches!(
                read_file(doubled.as_slice()),
                Err(ReadFileError::ChunkCount { tag: t, count: 2 }) if t == tag
            ),
            "{tag}"
        );
    }
}

#[test]
fn a_damaged_chunk_is_reported() {
    let bytes = write(&file());
    let cut = |tag| {
        rewrite(&bytes, |t, version, mut payload| {
            if t == tag {
                payload.pop();
            }
            vec![(t, version, payload)]
        })
    };
    assert!(matches!(
        read_file(cut(SUPPORTS_TAG).as_slice()),
        Err(ReadFileError::Supports(DecodeDocumentsError::Malformed))
    ));
    assert!(matches!(
        read_file(cut(HOLLOWS_TAG).as_slice()),
        Err(ReadFileError::Hollows(DecodeDocumentsError::Malformed))
    ));
    assert!(matches!(
        read_file(cut(MESH_TAG).as_slice()),
        Err(ReadFileError::Mesh(DecodeMeshError::Malformed))
    ));
    assert!(matches!(
        read_file(without(&bytes, MESH_TAG).as_slice()),
        Err(ReadFileError::Scene(DecodeSceneError::MeshCount {
            named: 3,
            found: 0
        }))
    ));
}

#[test]
fn a_reader_that_skips_the_application_chunks_sees_the_same_scene() {
    let file = file();
    let mut bytes = write(&file);
    for tag in [SETTINGS_TAG, SUPPORTS_TAG, HOLLOWS_TAG] {
        bytes = without(&bytes, tag);
    }
    let back = read_file(bytes.as_slice()).unwrap();
    // The supports leaf is still there with its empty mesh, and the hollowed
    // leaf with its hollow.
    assert_eq!(back.scene, file.scene);
    assert_eq!(back.settings, None);
    assert!(back.supports.is_empty() && back.hollows.is_empty());
}

#[test]
fn a_support_leaf_must_show_an_empty_mesh() {
    let mut file = file();
    file.supports = vec![document(1, "supports on a leaf with a mesh")];
    assert!(matches!(
        write_file(&file, Vec::new()),
        Err(WriteFileError::SupportMeshNotEmpty { leaf: 1 })
    ));
}

#[test]
fn documents_must_name_a_leaf() {
    let mut supports = file();
    supports.supports = vec![document(3, "supports on a group")];
    assert!(matches!(
        write_file(&supports, Vec::new()),
        Err(WriteFileError::Supports(EncodeDocumentsError::NotALeaf))
    ));
    let mut hollows = file();
    hollows.hollows = vec![document(9, "a hollow outside the scene")];
    assert!(matches!(
        write_file(&hollows, Vec::new()),
        Err(WriteFileError::Hollows(EncodeDocumentsError::NotALeaf))
    ));
}
