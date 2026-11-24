/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The files to convert, where each goes, and the conversion itself, apart
//! from the window, so it can be tested without one.

use std::collections::HashSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use bread_cli::{Credit, StlFile, from_stl_with, to_stl};
use bread_format::read_file;

/// Which way a file goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    ToBread,
    ToStl,
}

impl Way {
    /// The extension of what it makes.
    pub fn extension(self) -> &'static str {
        match self {
            Self::ToBread => "bread",
            Self::ToStl => "stl",
        }
    }
}

/// What a run writes into each `.bread` file it makes (`SPEC.md` 12): the
/// file's credit, its title among it, and the program and the time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub credit: Credit,
    pub created: String,
    pub generator: String,
}

/// The title a file is given unless it is changed: its name, without the
/// extension.
pub fn title_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Which files are converted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Each by its extension.
    #[default]
    Both,
    ToBread,
    ToStl,
}

/// What to do when the file to write is already there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhenThere {
    #[default]
    Skip,
    Replace,
    /// Write it beside, as `name (2).bread`.
    KeepBoth,
}

/// Where the files go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Output {
    #[default]
    NextTo,
    /// Into `dir`, under the subfolders each file was found in, if
    /// `keep_tree`.
    Folder { dir: PathBuf, keep_tree: bool },
}

/// A file to convert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub source: PathBuf,
    /// The folder it was found in, when it came with a folder: its
    /// subfolders are kept under an output folder.
    pub root: Option<PathBuf>,
    pub way: Way,
    pub size: u64,
}

/// The way a file goes, by its extension, if `direction` converts it.
pub fn way_of(path: &Path, direction: Direction) -> Option<Way> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let way = match ext.as_str() {
        "stl" => Way::ToBread,
        "bread" => Way::ToStl,
        _ => return None,
    };
    match (direction, way) {
        (Direction::Both, _)
        | (Direction::ToBread, Way::ToBread)
        | (Direction::ToStl, Way::ToStl) => Some(way),
        _ => None,
    }
}

/// The files `paths` name, with what folders hold (and their subfolders, if
/// `subfolders`), that `direction` converts, in name order within each
/// folder, leaving out those in `known`. Folders and files that cannot be
/// read are passed over.
pub fn collect(
    paths: &[PathBuf],
    subfolders: bool,
    direction: Direction,
    known: &HashSet<PathBuf>,
) -> Vec<Job> {
    let mut jobs = Vec::new();
    let mut seen = known.clone();
    let mut add = |path: PathBuf, root: Option<&Path>, jobs: &mut Vec<Job>| {
        let Some(way) = way_of(&path, direction) else {
            return;
        };
        let Ok(meta) = fs::metadata(&path) else {
            return;
        };
        if meta.is_file() && seen.insert(path.clone()) {
            jobs.push(Job {
                root: root.map(Path::to_path_buf),
                size: meta.len(),
                source: path,
                way,
            });
        }
    };
    for path in paths {
        if path.is_dir() {
            let mut stack = vec![path.clone()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                let mut entries: Vec<_> =
                    entries.filter_map(Result::ok).map(|e| e.path()).collect();
                entries.sort();
                let mut dirs = Vec::new();
                for entry in entries {
                    if entry.is_dir() {
                        if subfolders {
                            dirs.push(entry);
                        }
                    } else {
                        add(entry, Some(path), &mut jobs);
                    }
                }
                // Walked in name order: the last pushed is taken first.
                stack.extend(dirs.into_iter().rev());
            }
        } else {
            add(path.clone(), None, &mut jobs);
        }
    }
    jobs
}

/// Where a file would go, before looking at what is there.
pub fn target(job: &Job, output: &Output) -> PathBuf {
    let ext = job.way.extension();
    match output {
        Output::NextTo => job.source.with_extension(ext),
        Output::Folder { dir, keep_tree } => {
            let name = Path::new(job.source.file_name().unwrap_or_default()).with_extension(ext);
            let under = match (&job.root, keep_tree) {
                (Some(root), true) => job
                    .source
                    .parent()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
                _ => PathBuf::new(),
            };
            dir.join(under).join(name)
        }
    }
}

/// What to do with a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    Write(PathBuf),
    /// Not converted: the file it would write is there.
    Skip(PathBuf),
}

/// Where each job goes, by `output` and `when_there`. Two jobs never write
/// the same file, and no job writes over a file another job reads: such a
/// file is kept beside, whatever `when_there` says.
pub fn plan(jobs: &[Job], output: &Output, when_there: WhenThere) -> Vec<Plan> {
    let sources: HashSet<&Path> = jobs.iter().map(|j| j.source.as_path()).collect();
    let mut taken: HashSet<PathBuf> = HashSet::new();
    let free = |p: &Path, taken: &HashSet<PathBuf>| {
        !p.exists() && !taken.contains(p) && !sources.contains(p)
    };
    jobs.iter()
        .map(|job| {
            let want = target(job, output);
            let path = if free(&want, &taken) {
                want
            } else if when_there == WhenThere::Skip && !taken.contains(&want) {
                return Plan::Skip(want);
            } else if when_there == WhenThere::Replace
                && !taken.contains(&want)
                && !sources.contains(want.as_path())
            {
                want
            } else {
                beside(&want, |p| free(p, &taken))
            };
            taken.insert(path.clone());
            Plan::Write(path)
        })
        .collect()
}

/// `name (2).ext`, `name (3).ext`... the first that `free` takes.
fn beside(path: &Path, free: impl Fn(&Path) -> bool) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy())
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}).{ext}")))
        .find(|p| free(p))
        .expect("a free name")
}

/// Converts `job` in memory, checked as `bread from-stl` and `bread to-stl`
/// check it, and gives what it makes, without writing anything. A `.bread`
/// file carries `stamp`.
pub fn make(job: &Job, stamp: &Stamp) -> Result<Vec<u8>, String> {
    let bytes = fs::read(&job.source).map_err(|e| format!("cannot read it: {e}"))?;
    match job.way {
        Way::ToBread => {
            let name = job
                .source
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let prefix = format!("{name}: ");
            let metadata = stamp.credit.metadata(&stamp.created, &stamp.generator);
            Ok(from_stl_with(&[StlFile { name, bytes }], None, metadata)
                .map_err(|e| e.0.strip_prefix(&prefix).unwrap_or(&e.0).to_owned())?
                .0)
        }
        Way::ToStl => {
            let file = read_file(bytes.as_slice()).map_err(|e| e.to_string())?;
            let (stl, parts) = to_stl(&file).map_err(|e| e.to_string())?;
            if parts == 0 {
                return Err("it shows nothing to put in an STL".into());
            }
            Ok(stl)
        }
    }
}

/// Writes `bytes` to `out`: to a `.part` file beside it first, moved into
/// place once whole, so a stop or a crash never leaves half a file.
pub fn write(bytes: &[u8], out: &Path) -> Result<(), String> {
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    }
    let mut part = out.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = (|| {
        let mut f = fs::File::create(&part)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&part, out)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&part);
        return Err(format!("cannot write {}: {e}", out.display()));
    }
    Ok(())
}

/// Converts `job` into `out`, as [`make`] and [`write`] do. Gives the size
/// written.
pub fn convert(job: &Job, out: &Path, stamp: &Stamp) -> Result<u64, String> {
    let made = make(job, stamp)?;
    write(&made, out)?;
    Ok(made.len() as u64)
}

/// How much room is free on the drive that holds `path`, or would hold it:
/// the nearest folder above it that is there is asked.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut at = Some(path);
    while let Some(p) = at {
        if p.is_dir() {
            return fs4::available_space(p).ok();
        }
        at = p.parent();
    }
    None
}

#[cfg(test)]
mod tests;
