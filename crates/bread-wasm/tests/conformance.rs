/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `read_chunks` and its C ABI against the conformance files.

#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

use bread_format::{BreadReader, ChunkTag, supported_version};
use bread_wasm::abi::{
    bread_alloc, bread_chunk_count, bread_chunk_payload, bread_chunk_payload_len, bread_chunk_tag,
    bread_chunk_version, bread_free, bread_read_chunks, bread_refusal_chunk, bread_refusal_reason,
    bread_refusal_reason_len, bread_release,
};
use bread_wasm::{Refusal, read_chunks};

/// The reasons a container reader gives on its own; the other reasons are
/// for the readers of the chunks.
const CONTAINER_REASONS: [&str; 5] = ["not-bread", "version", "cut-short", "crc", "bad-payload"];

fn files(dir: &str) -> Vec<(String, Vec<u8>, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance")
        .join(dir);
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bread"))
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let json = std::fs::read_to_string(path.with_extension("json")).unwrap();
            (name, std::fs::read(&path).unwrap(), json)
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no files in {}", dir.display());
    files
}

/// The string value of the top-level `key` in a case's JSON, or `None` for
/// `null`.
fn field(json: &str, key: &str) -> Option<String> {
    let rest = &json[json.find(&format!("\"{key}\": ")).unwrap() + key.len() + 4..];
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"').unwrap()].to_owned())
}

fn tag(name: Option<String>) -> Option<ChunkTag> {
    name.map(|name| ChunkTag(name.as_bytes().try_into().unwrap()))
}

#[test]
fn every_valid_file_gives_its_known_chunks() {
    for (name, bytes, _) in files("valid") {
        let chunks = read_chunks(&bytes).unwrap_or_else(|err| panic!("{name}: {err:?}"));
        let mut reader = BreadReader::new(bytes.as_slice()).unwrap();
        let mut known = Vec::new();
        while let Some(chunk) = reader.next_chunk().unwrap() {
            if supported_version(chunk.tag).is_some() {
                known.push(chunk);
            }
        }
        assert_eq!(chunks, known, "{name}");
        assert!(chunks.iter().any(|chunk| chunk.tag.0 == *b"SCNE"), "{name}");
    }
}

#[test]
fn unknown_and_private_chunks_are_skipped() {
    for (name, bytes, _) in files("valid") {
        if name.contains("unknown") || name.contains("private") {
            let tags: Vec<_> = read_chunks(&bytes).unwrap().iter().map(|c| c.tag).collect();
            assert_eq!(
                tags,
                [ChunkTag(*b"SCNE"), ChunkTag(*b"MESH")],
                "{name} keeps only the scene and the mesh"
            );
        }
    }
}

#[test]
fn the_container_cases_are_refused_for_their_reason() {
    let mut refused = 0;
    for (name, bytes, json) in files("invalid") {
        let reason = field(&json, "reason").unwrap();
        let result = read_chunks(&bytes);
        if CONTAINER_REASONS.contains(&reason.as_str()) {
            let expected = Refusal {
                reason: CONTAINER_REASONS.iter().find(|&&r| r == reason).unwrap(),
                chunk: tag(field(&json, "chunk")),
            };
            assert_eq!(result, Err(expected), "{name}");
            refused += 1;
        } else {
            assert!(result.is_ok(), "{name} is for the chunk readers to refuse");
        }
    }
    assert_eq!(refused, 8, "the container cases");
}

/// Copies `bytes` in and reads them, as JavaScript does.
fn read_through_abi(bytes: &[u8]) -> u32 {
    let ptr = bread_alloc(bytes.len());
    // SAFETY: `ptr` holds `bytes.len()` bytes from `bread_alloc`, filled
    // here before they are read, and freed once.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let read = bread_read_chunks(ptr, bytes.len());
        bread_free(ptr, bytes.len());
        read
    }
}

fn conformance_file(path: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../conformance")
            .join(path),
    )
    .unwrap()
}

#[test]
fn the_abi_gives_the_chunks() {
    let bytes = conformance_file("valid/triangle.bread");
    assert_eq!(read_through_abi(&bytes), 1);
    let chunks = read_chunks(&bytes).unwrap();
    assert_eq!(bread_chunk_count(), chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        assert_eq!(bread_chunk_tag(index), u32::from_le_bytes(chunk.tag.0));
        assert_eq!(bread_chunk_version(index), u32::from(chunk.version));
        let len = bread_chunk_payload_len(index);
        // SAFETY: the payload of a chunk of the last read, still kept.
        let payload = unsafe { std::slice::from_raw_parts(bread_chunk_payload(index), len) };
        assert_eq!(payload, chunk.payload);
    }
    let past = chunks.len();
    assert_eq!(bread_chunk_tag(past), 0);
    assert_eq!(bread_chunk_payload_len(past), 0);
    assert!(bread_chunk_payload(past).is_null());
    assert!(bread_refusal_reason().is_null());

    bread_release();
    assert_eq!(bread_chunk_count(), 0);
}

#[test]
fn the_abi_gives_the_refusal() {
    assert_eq!(
        read_through_abi(&conformance_file("invalid/crc-mismatch.bread")),
        0
    );
    assert_eq!(bread_chunk_count(), 0);
    let len = bread_refusal_reason_len();
    // SAFETY: the reason of the last read, a static string.
    let reason = unsafe { std::slice::from_raw_parts(bread_refusal_reason(), len) };
    assert_eq!(reason, b"crc");
    assert_eq!(bread_refusal_chunk(), u32::from_le_bytes(*b"MESH"));

    assert_eq!(
        read_through_abi(&conformance_file("invalid/wrong-magic.bread")),
        0
    );
    assert_eq!(bread_refusal_chunk(), 0);
    bread_release();
    assert!(bread_refusal_reason().is_null());
}
