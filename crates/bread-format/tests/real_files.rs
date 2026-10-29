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

use std::time::Instant;

use bread_format::{read_file, write_file};

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

        let triangles: usize = file.scene.meshes.iter().map(|m| m.triangles.len()).sum();
        println!(
            "{}: {:.2} MB, {} meshes, {triangles} triangles, {} nodes, settings: {}, \
             {} support and {} hollow documents; open {:.3} s, save {:.3} s",
            path.display(),
            bytes.len() as f64 / 1e6,
            file.scene.meshes.len(),
            file.scene.nodes.len(),
            file.settings.is_some(),
            file.supports.len(),
            file.hollows.len(),
            open.as_secs_f64(),
            save.as_secs_f64(),
        );
        assert!(
            written == bytes,
            "{} comes back with other bytes",
            path.display()
        );
    }
}
