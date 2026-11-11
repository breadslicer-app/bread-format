/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, HOLLOWS_VERSION, HollowEntry,
    SETTINGS_TAG, SETTINGS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, SupportEntry, encode_hollows,
};
use crate::container::{BreadReader, BreadWriter, ChunkTag};
use crate::file::{
    BreadFile, ReadFileError, THUMBNAIL_TAG, THUMBNAIL_VERSION, WriteFileError, read_file,
    supported_version, write_file,
};
use crate::mesh::{DecodeMeshError, MESH_TAG, MESH_VERSION};
use crate::preview::{
    DecodePreviewsError, SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION, SupportPreview,
};
use crate::scene::{DecodeSceneError, SCENE_TAG, SCENE_VERSION};
use crate::surface::{SURFACE_TAG, SURFACE_VERSION, SurfaceEntry};
use crate::tests_preview::preview;
use crate::tests_scene::scene;
use crate::tests_surface::paint;

fn supports(model: usize, support: usize, text: &str) -> SupportEntry {
    SupportEntry {
        model,
        support,
        generator: "df".into(),
        version: 1,
        document: text.as_bytes().to_vec(),
    }
}

fn hollow(model: usize, original: usize, text: &str) -> HollowEntry {
    HollowEntry {
        model,
        original,
        generator: "df".into(),
        version: 1,
        document: text.as_bytes().to_vec(),
    }
}

/// The PNG signature and a few bytes that stand for the picture: the crate
/// never decodes it.
const THUMBNAIL: &[u8] = b"\x89PNG\r\n\x1a\nthe rest of a picture";

/// The test scene with a thumbnail and settings, and a hollowed model with
/// supports shown by the leaf with an empty mesh, their preview, and paint.
fn file() -> BreadFile {
    BreadFile {
        scene: scene(),
        settings: Some(b"version = 1\nprinter = \"example\"\n".to_vec()),
        supports: vec![supports(1, 2, "tips, trunks and rafts")],
        hollows: vec![hollow(1, 0, "settings and drains")],
        thumbnail: Some(THUMBNAIL.to_vec()),
        support_previews: vec![preview()],
        surface: vec![paint(0, 0), paint(1, 1), paint(1, 2)],
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
            THUMBNAIL_TAG,
            SCENE_TAG,
            SETTINGS_TAG,
            SUPPORTS_TAG,
            HOLLOWS_TAG,
            SURFACE_TAG,
            MESH_TAG,
            MESH_TAG,
            MESH_TAG,
            SUPPORT_PREVIEW_TAG
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
fn supported_versions_are_the_ones_written() {
    for (tag, version) in [
        (THUMBNAIL_TAG, THUMBNAIL_VERSION),
        (SCENE_TAG, SCENE_VERSION),
        (SETTINGS_TAG, SETTINGS_VERSION),
        (SUPPORTS_TAG, SUPPORTS_VERSION),
        (HOLLOWS_TAG, HOLLOWS_VERSION),
        (MESH_TAG, MESH_VERSION),
        (SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION),
        (SURFACE_TAG, SURFACE_VERSION),
    ] {
        assert_eq!(supported_version(tag), Some(version), "{tag}");
    }
    assert_eq!(supported_version(ChunkTag(*b"ZZZZ")), None);
    assert_eq!(supported_version(ChunkTag(*b"xmpl")), None);
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
    for (tag, supported) in [
        (THUMBNAIL_TAG, THUMBNAIL_VERSION),
        (SCENE_TAG, SCENE_VERSION),
        (SETTINGS_TAG, SETTINGS_VERSION),
        (SUPPORTS_TAG, SUPPORTS_VERSION),
        (HOLLOWS_TAG, HOLLOWS_VERSION),
        (MESH_TAG, MESH_VERSION),
        (SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION),
        (SURFACE_TAG, SURFACE_VERSION),
    ] {
        for bad in [0, supported + 1] {
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
fn draft_supports_and_hollows_are_skipped() {
    let file = file();
    // Version 1 entries named one leaf each, with no part.
    let draft = rewrite(&write(&file), |tag, version, payload| {
        if tag == SUPPORTS_TAG || tag == HOLLOWS_TAG {
            vec![(tag, 1, b"\x01\0\0\0\x02\0\0\0an older envelope".to_vec())]
        } else {
            vec![(tag, version, payload)]
        }
    });
    let back = read_file(draft.as_slice()).unwrap();
    assert_eq!(back.scene, file.scene);
    assert_eq!(back.settings, file.settings);
    assert!(back.supports.is_empty() && back.hollows.is_empty());
    // The previews of draft supports cannot be checked, so they go too.
    assert!(back.support_previews.is_empty());
    // A draft chunk still counts as the one supports chunk of the file.
    let doubled = rewrite(&write(&file), |tag, version, payload| {
        let mut chunks = vec![(tag, version, payload)];
        if tag == SUPPORTS_TAG {
            chunks.push((tag, 1, Vec::new()));
        }
        chunks
    });
    assert!(matches!(
        read_file(doubled.as_slice()),
        Err(ReadFileError::ChunkCount { tag, count: 2 }) if tag == SUPPORTS_TAG
    ));
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
    for tag in [
        THUMBNAIL_TAG,
        SCENE_TAG,
        SETTINGS_TAG,
        SUPPORTS_TAG,
        HOLLOWS_TAG,
        SUPPORT_PREVIEW_TAG,
    ] {
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
        read_file(cut(SUPPORT_PREVIEW_TAG).as_slice()),
        Err(ReadFileError::Previews(DecodePreviewsError::Malformed))
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
fn a_thumbnail_is_carried_without_being_read() {
    // Section 8.2: what the thumbnail holds never makes a file unreadable.
    for picture in [&b""[..], b"not a PNG at all"] {
        let mut file = file();
        file.thumbnail = Some(picture.to_vec());
        let bytes = write(&file);
        assert_eq!(read_file(bytes.as_slice()).unwrap(), file);
    }
    // Without one, the file has no thumbnail chunk and reads the same.
    let bytes = without(&write(&file()), THUMBNAIL_TAG);
    assert_eq!(tags(&bytes)[0], SCENE_TAG);
    let back = read_file(bytes.as_slice()).unwrap();
    assert_eq!(back.thumbnail, None);
    assert_eq!(back.scene, file().scene);
}

#[test]
fn a_reader_that_skips_the_application_chunks_sees_the_same_scene() {
    let file = file();
    let mut bytes = write(&file);
    for tag in [SETTINGS_TAG, SUPPORTS_TAG, HOLLOWS_TAG, SUPPORT_PREVIEW_TAG] {
        bytes = without(&bytes, tag);
    }
    let back = read_file(bytes.as_slice()).unwrap();
    // The support leaf is still there with its empty mesh, and the hollowed
    // model with its hollow.
    assert_eq!(back.scene, file.scene);
    assert_eq!(back.settings, None);
    assert!(back.supports.is_empty() && back.hollows.is_empty());
    assert!(back.support_previews.is_empty());
}

#[test]
fn a_support_leaf_must_show_an_empty_mesh() {
    let mut file = file();
    file.supports = vec![supports(0, 1, "supports on a leaf with a mesh")];
    assert!(matches!(
        write_file(&file, Vec::new()),
        Err(WriteFileError::SupportMeshNotEmpty { leaf: 1 })
    ));
}

#[test]
fn a_support_leaf_is_no_hollowed_model() {
    let mut broken = file();
    broken
        .hollows
        .push(hollow(2, 2, "a hollow of the supports"));
    assert!(matches!(
        write_file(&broken, Vec::new()),
        Err(WriteFileError::SupportIsHollowed { leaf: 2 })
    ));
    // Each chunk is fine on its own; only the two together break the rule.
    let hollows = encode_hollows(&broken.hollows, &broken.scene).unwrap();
    let bytes = rewrite(&write(&file()), |tag, version, payload| {
        if tag == HOLLOWS_TAG {
            vec![(tag, version, hollows.clone())]
        } else {
            vec![(tag, version, payload)]
        }
    });
    assert!(matches!(
        read_file(bytes.as_slice()),
        Err(ReadFileError::SupportIsHollowed { leaf: 2 })
    ));
}

#[test]
fn documents_must_name_a_leaf() {
    let mut with_supports = file();
    with_supports.supports = vec![supports(3, 2, "supports on a group")];
    assert!(matches!(
        write_file(&with_supports, Vec::new()),
        Err(WriteFileError::Supports(EncodeDocumentsError::NotALeaf))
    ));
    let mut hollows = file();
    hollows.hollows = vec![hollow(9, 0, "a hollow outside the scene")];
    assert!(matches!(
        write_file(&hollows, Vec::new()),
        Err(WriteFileError::Hollows(EncodeDocumentsError::NotALeaf))
    ));
}

#[test]
fn a_preview_shows_the_support_leaf_of_an_entry() {
    // Without supports, the support leaf is a leaf like any other.
    let mut lonely = file();
    lonely.supports.clear();
    assert!(matches!(
        write_file(&lonely, Vec::new()),
        Err(WriteFileError::PreviewWithoutSupports { leaf: 2 })
    ));
    let bytes = without(&write(&file()), SUPPORTS_TAG);
    assert!(matches!(
        read_file(bytes.as_slice()),
        Err(ReadFileError::PreviewWithoutSupports { leaf: 2 })
    ));
    // Some support leaves may go without a preview.
    let mut bare = file();
    bare.support_previews.clear();
    let bytes = write(&bare);
    assert!(!tags(&bytes).contains(&SUPPORT_PREVIEW_TAG));
    assert_eq!(read_file(bytes.as_slice()).unwrap(), bare);
}

#[test]
fn a_reader_that_skips_the_preview_sees_the_same_file() {
    let file = file();
    let back = read_file(without(&write(&file), SUPPORT_PREVIEW_TAG).as_slice()).unwrap();
    assert_eq!(
        back,
        BreadFile {
            support_previews: Vec::<SupportPreview>::new(),
            ..file
        }
    );
}

#[test]
fn a_reader_that_skips_the_paint_sees_the_same_file() {
    let file = file();
    let back = read_file(without(&write(&file), SURFACE_TAG).as_slice()).unwrap();
    assert_eq!(
        back,
        BreadFile {
            surface: Vec::<SurfaceEntry>::new(),
            ..file
        }
    );
}

#[test]
fn paint_on_a_part_that_is_not_a_leaf_is_refused() {
    let mut file = file();
    file.surface.push(paint(3, 0));
    assert!(matches!(
        write_file(&file, Vec::new()),
        Err(WriteFileError::Surface(_))
    ));
}

#[test]
fn two_paint_chunks_are_refused() {
    let bytes = rewrite(&write(&file()), |tag, version, payload| {
        let copies = if tag == SURFACE_TAG { 2 } else { 1 };
        vec![(tag, version, payload); copies]
    });
    assert!(matches!(
        read_file(bytes.as_slice()),
        Err(ReadFileError::ChunkCount {
            tag: SURFACE_TAG,
            count: 2
        })
    ));
}
