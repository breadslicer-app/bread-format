/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use bread_format::{read_file, read_stl};

use super::{Direction, Job, Output, Plan, Way, WhenThere, collect, convert, plan, target};

/// A folder of its own for each test, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("bread-converter-tests")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A binary STL of one triangle, moved by `x`.
fn stl(x: f32) -> Vec<u8> {
    let mut out = vec![0; 80];
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    for corner in [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.1]] {
        for v in corner {
            out.extend_from_slice(&f32::to_le_bytes(v));
        }
    }
    out.extend_from_slice(&[0; 2]);
    out
}

/// root/a.stl, root/b.bread (made from a.stl), root/notes.txt,
/// root/sub/c.stl, root/sub/deeper/d.STL.
fn tree(name: &str) -> PathBuf {
    let root = scratch(name);
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::write(root.join("a.stl"), stl(0.0)).unwrap();
    fs::write(root.join("notes.txt"), "hello").unwrap();
    fs::write(root.join("sub/c.stl"), stl(2.0)).unwrap();
    fs::write(root.join("sub/deeper/d.STL"), stl(3.0)).unwrap();
    let a = Job {
        source: root.join("a.stl"),
        root: None,
        way: Way::ToBread,
        size: 134,
    };
    convert(&a, &root.join("b.bread")).unwrap();
    root
}

fn names(jobs: &[Job], root: &Path) -> Vec<String> {
    jobs.iter()
        .map(|j| {
            j.source
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

#[test]
fn folders_give_their_files_in_name_order_by_direction() {
    let root = tree("collect");
    let none = HashSet::new();
    let all = collect(std::slice::from_ref(&root), true, Direction::Both, &none);
    assert_eq!(
        names(&all, &root),
        ["a.stl", "b.bread", "sub/c.stl", "sub/deeper/d.STL"]
    );
    assert!(
        all.iter()
            .all(|j| j.root.as_deref() == Some(root.as_path()))
    );
    assert_eq!(all[0].size, 134);

    let flat = collect(std::slice::from_ref(&root), false, Direction::Both, &none);
    assert_eq!(names(&flat, &root), ["a.stl", "b.bread"]);
    let to_stl = collect(std::slice::from_ref(&root), true, Direction::ToStl, &none);
    assert_eq!(names(&to_stl, &root), ["b.bread"]);
    let to_bread = collect(std::slice::from_ref(&root), true, Direction::ToBread, &none);
    assert_eq!(
        names(&to_bread, &root),
        ["a.stl", "sub/c.stl", "sub/deeper/d.STL"]
    );

    // A file already in the list, or given twice, comes once; a file given
    // alone has no folder.
    let known: HashSet<PathBuf> = [root.join("a.stl")].into();
    let again = collect(
        &[
            root.join("sub/c.stl"),
            root.join("sub/c.stl"),
            root.join("a.stl"),
            root.join("notes.txt"),
        ],
        true,
        Direction::Both,
        &known,
    );
    assert_eq!(names(&again, &root), ["sub/c.stl"]);
    assert_eq!(again[0].root, None);
}

#[test]
fn files_go_next_to_themselves_or_into_a_folder_with_or_without_their_subfolders() {
    let root = tree("target");
    let jobs = collect(
        std::slice::from_ref(&root),
        true,
        Direction::Both,
        &HashSet::new(),
    );
    let d = &jobs[3];
    assert_eq!(target(d, &Output::NextTo), root.join("sub/deeper/d.bread"));
    let out = root.join("out");
    let kept = Output::Folder {
        dir: out.clone(),
        keep_tree: true,
    };
    assert_eq!(
        target(d, &kept),
        out.join("sub").join("deeper").join("d.bread")
    );
    let flat = Output::Folder {
        dir: out.clone(),
        keep_tree: false,
    };
    assert_eq!(target(d, &flat), out.join("d.bread"));
    assert_eq!(target(&jobs[1], &flat), out.join("b.stl"));
}

#[test]
fn a_file_that_is_there_is_skipped_replaced_or_kept_beside() {
    let root = tree("plan");
    fs::write(root.join("sub/c.bread"), "there").unwrap();
    let jobs = collect(
        &[root.join("sub")],
        true,
        Direction::ToBread,
        &HashSet::new(),
    );
    let c = root.join("sub/c.bread");
    assert_eq!(
        plan(&jobs, &Output::NextTo, WhenThere::Skip)[0],
        Plan::Skip(c.clone())
    );
    assert_eq!(
        plan(&jobs, &Output::NextTo, WhenThere::Replace)[0],
        Plan::Write(c)
    );
    assert_eq!(
        plan(&jobs, &Output::NextTo, WhenThere::KeepBoth)[0],
        Plan::Write(root.join("sub/c (2).bread"))
    );
}

#[test]
fn no_two_files_write_the_same_and_none_writes_over_what_another_reads() {
    let root = tree("collide");
    // Both a.stl and b.bread in the list: a.stl would make a.bread, b.bread
    // makes b.stl. Make them meet: a copy of b.bread named a.bread.
    fs::copy(root.join("b.bread"), root.join("a.bread")).unwrap();
    let jobs = collect(
        std::slice::from_ref(&root),
        false,
        Direction::Both,
        &HashSet::new(),
    );
    assert_eq!(names(&jobs, &root), ["a.bread", "a.stl", "b.bread"]);
    // a.bread → a.stl, which a.stl is: kept beside even when replacing; and
    // a.stl → a.bread, which a.bread is.
    let plans = plan(&jobs, &Output::NextTo, WhenThere::Replace);
    assert_eq!(plans[0], Plan::Write(root.join("a (2).stl")));
    assert_eq!(plans[1], Plan::Write(root.join("a (2).bread")));
    assert_eq!(plans[2], Plan::Write(root.join("b.stl")));

    // Flattened into one folder, two files of one name do not meet.
    fs::copy(root.join("a.stl"), root.join("sub/a.stl")).unwrap();
    let jobs = collect(
        std::slice::from_ref(&root),
        true,
        Direction::ToBread,
        &HashSet::new(),
    );
    let out = root.join("out");
    let flat = Output::Folder {
        dir: out.clone(),
        keep_tree: false,
    };
    let plans = plan(&jobs, &flat, WhenThere::Skip);
    assert_eq!(plans[0], Plan::Write(out.join("a.bread")));
    assert_eq!(names(&jobs, &root)[1], "sub/a.stl");
    assert_eq!(plans[1], Plan::Write(out.join("a (2).bread")));
}

#[test]
fn conversions_go_both_ways_and_leave_no_part_behind() {
    let root = tree("convert");
    let jobs = collect(
        std::slice::from_ref(&root),
        true,
        Direction::Both,
        &HashSet::new(),
    );
    let out = root.join("out");
    let kept = Output::Folder {
        dir: out.clone(),
        keep_tree: true,
    };
    for (job, p) in jobs.iter().zip(plan(&jobs, &kept, WhenThere::Skip)) {
        let Plan::Write(to) = p else { panic!("{p:?}") };
        convert(job, &to).unwrap();
    }
    let d = read_file(fs::read(out.join("sub/deeper/d.bread")).unwrap().as_slice()).unwrap();
    assert_eq!(d.scene.meshes[0], read_stl(&stl(3.0)).unwrap());
    assert_eq!(fs::read(out.join("b.stl")).unwrap()[84..], stl(0.0)[84..]);
    let parts: Vec<_> = walk(&out)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "part"))
        .collect();
    assert!(parts.is_empty(), "{parts:?}");

    // What cannot be read fails with why, and writes nothing.
    let bad = root.join("bad.stl");
    fs::write(&bad, "not an stl").unwrap();
    let job = Job {
        source: bad,
        root: None,
        way: Way::ToBread,
        size: 10,
    };
    let err = convert(&job, &out.join("bad.bread")).unwrap_err();
    assert!(err.contains("not an STL"), "{err}");
    assert!(!out.join("bad.bread").exists());
    assert!(!out.join("bad.bread.part").exists());
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
