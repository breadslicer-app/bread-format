/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The `bread` program, run as a person runs it, on the conformance files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BREAD: &str = env!("CARGO_BIN_EXE_bread");

fn conformance(kind: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance")
        .join(kind);
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bread"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

fn bread(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(BREAD).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A folder of its own for each test, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_reads_every_valid_file_and_refuses_every_invalid_one() {
    for path in conformance("valid") {
        let out = bread(&["check".as_ref(), path.as_os_str()]);
        assert!(out.status.success(), "{}: {}", path.display(), text(&out));
        assert!(text(&out).starts_with("reads"));
    }
    for path in conformance("invalid") {
        let out = bread(&["check".as_ref(), path.as_os_str()]);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{}: {}",
            path.display(),
            text(&out)
        );
        assert!(text(&out).starts_with("refused"));
    }
}

#[test]
fn info_says_what_a_file_holds() {
    for path in conformance("valid") {
        let out = bread(&["info".as_ref(), path.as_os_str()]);
        assert!(out.status.success(), "{}: {}", path.display(), text(&out));
        assert!(text(&out).contains("SCNE v1"), "{}", text(&out));
    }
    let bad = conformance("invalid").remove(0);
    assert_eq!(
        bread(&["info".as_ref(), bad.as_os_str()]).status.code(),
        Some(1)
    );
}

/// The corners of a binary STL, as bits, in order.
fn corners(stl: &[u8]) -> Vec<u32> {
    let (triangles, _) = stl[84..].as_chunks::<50>();
    triangles
        .iter()
        .flat_map(|t| t[12..48].as_chunks::<4>().0)
        .map(|x| u32::from_le_bytes(*x))
        .collect()
}

#[test]
fn every_valid_file_goes_to_stl_and_back() {
    let dir = scratch("round-trip");
    for path in conformance("valid") {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let stl = dir.join(format!("{stem}.stl"));
        let back = dir.join(format!("{stem}.bread"));
        let out = bread(&[
            "to-stl".as_ref(),
            path.as_os_str(),
            "-o".as_ref(),
            stl.as_os_str(),
        ]);
        assert!(out.status.success(), "{stem}: {}", text(&out));
        let first = fs::read(&stl).unwrap();
        if first.len() == 84 {
            continue; // shows nothing
        }
        let out = bread(&[
            "from-stl".as_ref(),
            stl.as_os_str(),
            "-o".as_ref(),
            back.as_os_str(),
        ]);
        assert!(out.status.success(), "{stem}: {}", text(&out));
        assert!(text(&out).contains("every corner checked"));
        // And out again: the same corners, bit for bit.
        let again = dir.join(format!("{stem}-again.stl"));
        let out = bread(&[
            "to-stl".as_ref(),
            back.as_os_str(),
            "-o".as_ref(),
            again.as_os_str(),
        ]);
        assert!(out.status.success(), "{stem}: {}", text(&out));
        assert_eq!(
            corners(&fs::read(&again).unwrap()),
            corners(&first),
            "{stem}"
        );
    }
}

#[test]
fn files_alone_convert_by_their_extension_next_to_themselves() {
    let dir = scratch("bare");
    let cube = dir.join("cube.bread");
    fs::copy(
        conformance("valid")
            .into_iter()
            .find(|p| p.ends_with("cube.bread"))
            .unwrap(),
        &cube,
    )
    .unwrap();
    let out = bread(&[cube.as_os_str()]);
    assert!(out.status.success(), "{}", text(&out));
    let stl = dir.join("cube.stl");
    assert!(stl.exists());

    // Not over a file that is there, unless told.
    let out = bread(&[stl.as_os_str()]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("already there"), "{}", text(&out));
    let before = fs::read(&cube).unwrap();
    let out = bread(&[stl.as_os_str(), "--force".as_ref()]);
    assert!(out.status.success(), "{}", text(&out));
    assert_ne!(fs::read(&cube).unwrap(), before, "written over");

    let notes = dir.join("notes.txt");
    fs::write(&notes, "hello").unwrap();
    assert_eq!(bread(&[notes.as_os_str()]).status.code(), Some(1));
}

#[test]
fn wrong_arguments_say_how() {
    assert_eq!(bread(&["--nope".as_ref()]).status.code(), Some(2));
    assert_eq!(bread(&["to-stl".as_ref()]).status.code(), Some(2));
    assert_eq!(
        bread(&["to-stl".as_ref(), "a".as_ref(), "b".as_ref()])
            .status
            .code(),
        Some(2)
    );
    let help = bread(&["--help".as_ref()]);
    assert!(help.status.success());
    assert!(text(&help).contains("from-stl"));
}
