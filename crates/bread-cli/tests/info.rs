/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `bread info` on the conformance files with roles and metadata: who made a
//! file first, then which leaves are not models, and every entry kept.

use std::path::Path;
use std::process::Command;

const BREAD: &str = env!("CARGO_BIN_EXE_bread");

/// What `bread info` prints for a valid conformance file, line by line,
/// without the indent.
fn info(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/valid")
        .join(name);
    let out = Command::new(BREAD).arg("info").arg(&path).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.trim_start().to_owned())
        .collect()
}

#[test]
fn the_creators_credit_comes_first() {
    let lines = info("metadata.bread");
    assert_eq!(
        lines[..4],
        [
            "title:    Box with a lid",
            "designer: A. Designer",
            "license:  CC-BY-4.0",
            "source:   https://example.com/box",
        ]
    );
    assert!(lines[4].ends_with(" bytes"), "{lines:?}");
}

#[test]
fn every_metadata_entry_is_listed_by_what_it_is_about() {
    let lines = info("metadata.bread");
    let at = lines.iter().position(|l| l == "metadata:").unwrap();
    assert_eq!(
        lines[at..],
        [
            "metadata:",
            "about the file:",
            "title: Box with a lid",
            "description: A test box, in two parts.",
            "designer: A. Designer",
            "copyright: \u{a9} 2026 A. Designer",
            "license: CC-BY-4.0",
            "source: https://example.com/box",
            "created: 2026-11-21T14:30:00Z",
            "generator: bread-format conformance",
            "uuid: 1b4e28ba-2fa1-41d2-883f-0a407f2b5c01",
            "about node 0 (base):",
            "title: Base",
            "uuid: 1b4e28ba-2fa1-41d2-883f-0a407f2b5c02",
            "about node 1 (lid \u{2014} top):",
            "uuid: 1b4e28ba-2fa1-41d2-883f-0a407f2b5c03",
            "example.note: 4 bytes: 00 01 02 ff",
        ]
    );
}

#[test]
fn each_leaf_that_is_not_a_model_is_named_with_its_role() {
    let lines = info("roles.bread");
    let at = lines
        .iter()
        .position(|l| l == "roles (leaves that are not models):")
        .unwrap();
    assert_eq!(
        lines[at + 1..],
        [
            "node 1 (supports): supports, for its supports entry",
            "node 2 (posts): supports",
            "node 3 (plate): a raft",
        ]
    );
    assert!(!lines.iter().any(|l| l == "metadata:"));
}

#[test]
fn entries_a_reader_leaves_out_are_not_shown() {
    let lines = info("metadata-ignored.bread");
    assert!(lines[0].ends_with(" bytes"), "{lines:?}");
    assert!(!lines.iter().any(|l| l == "metadata:"), "{lines:?}");
}
