/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Measures `.bread` on a folder of binary STL files.
//!
//! ```text
//! corpus-bench <folder> <out.json> [--limit N]
//! ```
//!
//! For each STL, in name order, it:
//!
//! 1. welds the triangle corners into vertices, two corners being one vertex
//!    when their coordinates are bit for bit equal, numbered by first use;
//! 2. writes a `.bread` file of one leaf showing that mesh, in memory, and
//!    times it;
//! 3. reads it back three times and keeps the fastest;
//! 4. checks that every corner of every triangle comes back bit for bit, in
//!    the STL's order.
//!
//! Each model's row goes to `<out.json>.partial` as soon as it is measured,
//! one JSON object per line, so a run that stops can go on where it left:
//! run it again, and the models already there are skipped. At the end it
//! writes the JSON array of every row to `<out.json>` and prints a summary.
//! Models are measured one at a time, so each gets the whole machine.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bread_format::{BreadFile, Mesh, Node, NodeKind, Scene, read_file, write_file};

/// One model's measurements.
struct Row {
    name: String,
    triangles: usize,
    vertices: usize,
    stl_bytes: u64,
    bread_bytes: usize,
    save_ms: f64,
    open_ms: f64,
    bit_exact: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(folder), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: corpus-bench <folder> <out.json> [--limit N]");
        std::process::exit(2);
    };
    let limit = match args.iter().position(|a| a == "--limit") {
        Some(i) => args
            .get(i + 1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(usize::MAX),
        None => usize::MAX,
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("the folder can be read")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("stl")))
        .collect();
    paths.sort();
    paths.truncate(limit);

    let partial = format!("{out}.partial");
    let mut lines: Vec<String> = std::fs::read_to_string(&partial)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    let done: std::collections::HashSet<String> = lines.iter().filter_map(|l| name_of(l)).collect();
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial)
        .expect("the partial output can be written");
    let started = Instant::now();
    for (i, path) in paths.iter().enumerate() {
        if done.contains(&stem(path)) {
            continue;
        }
        match measure(path) {
            Ok(row) => {
                eprintln!(
                    "[{}/{}] {} {} triangles {:.1}x save {:.0} ms open {:.0} ms{}",
                    i + 1,
                    paths.len(),
                    row.name,
                    row.triangles,
                    row.stl_bytes as f64 / row.bread_bytes as f64,
                    row.save_ms,
                    row.open_ms,
                    if row.bit_exact { "" } else { " NOT BIT EXACT" },
                );
                let line = row_json(&row);
                writeln!(log, "{line}").expect("the partial output can be written");
                log.flush().expect("the partial output can be written");
                lines.push(line);
            }
            Err(err) => eprintln!(
                "[{}/{}] {}: skipped, {err}",
                i + 1,
                paths.len(),
                path.display()
            ),
        }
    }
    lines.sort_by_key(|l| name_of(l));
    let body = lines.join(",\n  ");
    std::fs::write(out, format!("[\n  {body}\n]\n")).expect("the output can be written");
    summary(&lines, started.elapsed().as_secs_f64());
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The text of field `key` in one of this tool's own rows.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("\"{key}\": "))? + key.len() + 4;
    let rest = &line[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// The model name of one of this tool's own rows.
fn name_of(line: &str) -> Option<String> {
    let start = line.find("\"name\": \"")? + 9;
    let mut name = String::new();
    let mut chars = line[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(name),
            '\\' => name.push(chars.next()?),
            c => name.push(c),
        }
    }
    None
}

fn measure(path: &Path) -> Result<Row, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let corners = read_stl(&bytes)?;
    let mesh = weld(&corners);
    let (triangles, vertices) = (mesh.triangles.len(), mesh.vertices.len());
    let file = one_leaf(mesh);

    let t = Instant::now();
    let written = write_file(&file, Vec::new()).map_err(|e| e.to_string())?;
    let save_ms = t.elapsed().as_secs_f64() * 1e3;

    let mut open_ms = f64::MAX;
    let mut read = None;
    for _ in 0..3 {
        let t = Instant::now();
        let back = read_file(written.as_slice()).map_err(|e| e.to_string())?;
        open_ms = open_ms.min(t.elapsed().as_secs_f64() * 1e3);
        read = Some(back);
    }
    let read = read.expect("read three times");
    let bit_exact = read.scene.meshes.len() == 1 && same_corners(&read.scene.meshes[0], &corners);

    Ok(Row {
        name: stem(path),
        triangles,
        vertices,
        stl_bytes: bytes.len() as u64,
        bread_bytes: written.len(),
        save_ms,
        open_ms,
        bit_exact,
    })
}

/// The corners of a binary STL, as the bits of their coordinates: three per
/// triangle, in order. The facet normals and attribute bytes are not kept.
fn read_stl(bytes: &[u8]) -> Result<Vec<[u32; 3]>, String> {
    let count = bytes
        .get(80..84)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)
        .ok_or("shorter than an STL header")?;
    if bytes.len() != 84 + 50 * count {
        return Err("not a binary STL of the length its count says".into());
    }
    let mut corners = Vec::with_capacity(3 * count);
    for facet in bytes[84..].as_chunks::<50>().0 {
        for corner in 0..3 {
            let at = 12 + 12 * corner;
            corners.push(std::array::from_fn(|i| {
                let b = &facet[at + 4 * i..at + 4 * i + 4];
                u32::from_le_bytes(b.try_into().expect("4 bytes"))
            }));
        }
    }
    Ok(corners)
}

/// The corners welded into a mesh, as an STL importer welds them: equal bits,
/// one vertex, numbered by first use.
fn weld(corners: &[[u32; 3]]) -> Mesh {
    let mut index = std::collections::HashMap::with_capacity(corners.len() / 2);
    let mut vertices = Vec::new();
    let ids: Vec<u32> = corners
        .iter()
        .map(|c| {
            *index.entry(*c).or_insert_with(|| {
                vertices.push(c.map(f32::from_bits));
                (vertices.len() - 1) as u32
            })
        })
        .collect();
    let triangles = ids.as_chunks::<3>().0.to_vec();
    Mesh {
        meta: None,
        vertices,
        triangles,
    }
}

fn one_leaf(mesh: Mesh) -> BreadFile {
    BreadFile {
        scene: Scene {
            meta: None,
            meshes: vec![mesh],
            mesh_resources: Vec::new(),
            nodes: vec![Node {
                meta: None,
                transform: None,
                kind: NodeKind::Leaf { mesh: 0 },
            }],
            subtrees: Vec::new(),
            roots: vec![0],
        },
        settings: None,
        supports: Vec::new(),
        hollows: Vec::new(),
        thumbnail: None,
        support_previews: Vec::new(),
        surface: Vec::new(),
    }
}

/// Whether `mesh` gives back the STL's corners, bit for bit and in order.
fn same_corners(mesh: &Mesh, corners: &[[u32; 3]]) -> bool {
    mesh.triangles.len() * 3 == corners.len()
        && mesh
            .triangles
            .as_flattened()
            .iter()
            .zip(corners)
            .all(|(&v, corner)| {
                mesh.vertices
                    .get(v as usize)
                    .is_some_and(|p| p.map(f32::to_bits) == *corner)
            })
}

fn row_json(r: &Row) -> String {
    format!(
        "{{\"name\": {}, \"triangles\": {}, \"vertices\": {}, \"stl_bytes\": {}, \
         \"bread_bytes\": {}, \"save_ms\": {:.1}, \"open_ms\": {:.1}, \"bit_exact\": {}}}",
        quote(&r.name),
        r.triangles,
        r.vertices,
        r.stl_bytes,
        r.bread_bytes,
        r.save_ms,
        r.open_ms,
        r.bit_exact,
    )
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn summary(lines: &[String], seconds: f64) {
    let number = |l: &String, key| {
        field(l, key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let stl: f64 = lines.iter().map(|l| number(l, "stl_bytes")).sum();
    let bread: f64 = lines.iter().map(|l| number(l, "bread_bytes")).sum();
    let exact = lines
        .iter()
        .filter(|l| field(l, "bit_exact") == Some("true"))
        .count();
    let mut ratios: Vec<f64> = lines
        .iter()
        .map(|l| number(l, "stl_bytes") / number(l, "bread_bytes"))
        .collect();
    ratios.sort_by(f64::total_cmp);
    let median = ratios.get(ratios.len() / 2).copied().unwrap_or(0.0);
    eprintln!(
        "{} models, {exact} bit for bit; STL {:.2} GB, .bread {:.2} GB, {:.1}x overall; \
         per model {:.1}x to {:.1}x, median {median:.1}x; {seconds:.0} s",
        lines.len(),
        stl / 1e9,
        bread / 1e9,
        stl / bread,
        ratios.first().copied().unwrap_or(0.0),
        ratios.last().copied().unwrap_or(0.0),
    );
}
