/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Opens real `.bread` files, writes them back and checks that the same file
//! comes out, timing both. The files stay outside the repository; their paths
//! go in `BREAD_FILES`, separated as in `PATH` (`;` on Windows, `:`
//! elsewhere):
//!
//! ```text
//! BREAD_FILES=a.bread;b.bread cargo test --release -p bread-format
//!     --test real_files -- --ignored --nocapture
//! ```
//!
//! Meshes are written as version 2 whatever version they were read in, so
//! they must come back bit for bit rather than as the same bytes; every other
//! chunk must come back as it was. A file with draft supports or hollows
//! chunks (version 1) loses them on the way, since the reader skips them; the
//! skipped ones are counted. Writing what was written must give the same
//! bytes again.

use std::time::Instant;

use bread_format::{
    BreadFile, BreadReader, Chunk, HOLLOWS_TAG, HOLLOWS_VERSION, MESH_TAG, SUPPORTS_TAG,
    SUPPORTS_VERSION, read_file, write_file,
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

/// Every mesh of `a` and `b` the same, coordinates bit for bit.
fn same_meshes(a: &BreadFile, b: &BreadFile) -> bool {
    a.scene.meshes.len() == b.scene.meshes.len()
        && a.scene.meshes.iter().zip(&b.scene.meshes).all(|(x, y)| {
            x.meta == y.meta
                && x.triangles == y.triangles
                && x.vertices.len() == y.vertices.len()
                && x.vertices
                    .iter()
                    .zip(&y.vertices)
                    .all(|(p, q)| (0..3).all(|i| p[i].to_bits() == q[i].to_bits()))
        })
}

#[test]
#[ignore = "needs BREAD_FILES"]
fn real_files_come_back_the_same() {
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
        let back = read_file(written.as_slice()).expect("the written file opens");
        let triangles: usize = file.scene.meshes.iter().map(|m| m.triangles.len()).sum();
        println!(
            "{}: {:.2} MB, {} meshes, {triangles} triangles, {} nodes, settings: {}, \
             {} support and {} hollow entries, {} draft chunks skipped; \
             {:.2} MB written; open {:.3} s, save {:.3} s",
            path.display(),
            bytes.len() as f64 / 1e6,
            file.scene.meshes.len(),
            file.scene.nodes.len(),
            file.settings.is_some(),
            file.supports.len(),
            file.hollows.len(),
            drafts.len(),
            written.len() as f64 / 1e6,
            open.as_secs_f64(),
            save.as_secs_f64(),
        );
        assert!(
            same_meshes(&back, &file),
            "{} comes back with other meshes",
            path.display()
        );
        let not_meshes = |chunks: Vec<Chunk>| -> Vec<Chunk> {
            chunks.into_iter().filter(|c| c.tag != MESH_TAG).collect()
        };
        assert!(
            not_meshes(chunks(&written)) == not_meshes(kept),
            "{} comes back with other chunks",
            path.display()
        );
        assert!(
            write_file(&back, Vec::new()).expect("the file writes again") == written,
            "{} writes other bytes the second time",
            path.display()
        );
    }
}
