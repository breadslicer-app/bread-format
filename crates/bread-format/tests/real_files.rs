/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Opens real `.bread` files, writes them back and checks that the same bytes
//! come out, timing both. The files stay outside the repository; their paths
//! go in `BREAD_FILES`, separated as in `PATH` (`;` on Windows, `:`
//! elsewhere):
//!
//! ```text
//! BREAD_FILES=a.bread;b.bread cargo test --release -p bread-format
//!     --test real_files -- --ignored --nocapture
//! ```
//!
//! A file with draft supports or hollows chunks (version 1) loses them on the
//! way, since the reader skips them: for such a file every other chunk must
//! come back the same, and the skipped ones are counted.

use std::time::Instant;

use bread_format::{
    BreadReader, Chunk, HOLLOWS_TAG, HOLLOWS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, read_file,
    write_file,
};

fn chunks(bytes: &[u8]) -> Vec<Chunk> {
    let mut reader = BreadReader::new(bytes).expect("the file has a header");
    let mut chunks = Vec::new();
    while let Some(chunk) = reader.next_chunk().expect("the chunk reads") {
        chunks.push(chunk);
    }
    chunks
}

/// A supports or hollows chunk older than the one this build reads.
fn is_draft(chunk: &Chunk) -> bool {
    (chunk.tag == SUPPORTS_TAG && chunk.version < SUPPORTS_VERSION)
        || (chunk.tag == HOLLOWS_TAG && chunk.version < HOLLOWS_VERSION)
}

#[test]
#[ignore = "needs BREAD_FILES"]
fn real_files_come_back_byte_for_byte() {
    let Some(paths) = std::env::var_os("BREAD_FILES") else {
        return;
    };
    for path in std::env::split_paths(&paths) {
        let bytes = std::fs::read(&path).expect("the file reads");
        let started = Instant::now();
        let file = read_file(bytes.as_slice()).expect("the file opens");
        let open = started.elapsed();
        let started = Instant::now();
        let written = write_file(&file, Vec::new()).expect("the file writes");
        let save = started.elapsed();

        let (drafts, kept): (Vec<_>, Vec<_>) = chunks(&bytes).into_iter().partition(is_draft);
        let triangles: usize = file.scene.meshes.iter().map(|m| m.triangles.len()).sum();
        println!(
            "{}: {:.2} MB, {} meshes, {triangles} triangles, {} nodes, settings: {}, \
             {} support and {} hollow entries, {} draft chunks skipped; \
             open {:.3} s, save {:.3} s",
            path.display(),
            bytes.len() as f64 / 1e6,
            file.scene.meshes.len(),
            file.scene.nodes.len(),
            file.settings.is_some(),
            file.supports.len(),
            file.hollows.len(),
            drafts.len(),
            open.as_secs_f64(),
            save.as_secs_f64(),
        );
        if drafts.is_empty() {
            assert!(
                written == bytes,
                "{} comes back with other bytes",
                path.display()
            );
        } else {
            assert!(
                chunks(&written) == kept,
                "{} comes back with other chunks",
                path.display()
            );
        }
    }
}
