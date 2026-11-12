/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! STL in: binary and ASCII STL files read into a [`Mesh`], to be written as
//! `.bread`.
//!
//! An STL is a list of triangles, each with its three corners written out in
//! full. [`read_stl`] welds them into vertices: two corners are one vertex when
//! their coordinates are equal bit for bit, and vertices are numbered by first
//! use. Nothing is rounded or merged beyond that, so every corner of every
//! triangle comes back from the mesh exactly, in the STL's order. Facet normals
//! and attribute bytes are not kept: they are file data, not geometry.
//!
//! The text that comes with the triangles is kept as the mesh's metadata: an
//! ASCII STL's solid name as its name, and a binary STL's 80-byte header as
//! its comment, when they hold text.

use std::collections::HashMap;
use std::fmt;

use crate::mesh::Mesh;
use crate::meta::Meta;

/// Bytes before a binary STL's triangles: an 80-byte header and a `u32`
/// triangle count.
const BINARY_HEADER: usize = 84;
/// Bytes per triangle of a binary STL: normal, three corners, attribute.
const BINARY_TRIANGLE: usize = 50;

/// Why an STL could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadStlError {
    /// Neither a binary STL of the length its triangle count says, nor text
    /// that starts with `solid`.
    NotStl,
    /// An ASCII STL that breaks its grammar on this line, counted from 1.
    Ascii { line: usize },
    /// More vertices than a mesh can number with a `u32`.
    TooLarge,
}

impl fmt::Display for ReadStlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotStl => f.write_str("not an STL file"),
            Self::Ascii { line } => write!(f, "the ASCII STL is malformed on line {line}"),
            Self::TooLarge => f.write_str("the STL has too many vertices"),
        }
    }
}

impl std::error::Error for ReadStlError {}

/// Reads a binary or an ASCII STL, and welds its corners into a mesh.
///
/// A file is binary when its length is exactly what its triangle count says,
/// even if its header starts with `solid`, as many binary headers do; else it
/// is read as ASCII.
pub fn read_stl(bytes: &[u8]) -> Result<Mesh, ReadStlError> {
    let (corners, meta) = match binary_triangle_count(bytes) {
        Some(count) => (binary_corners(bytes, count), binary_meta(bytes)),
        None => ascii(bytes)?,
    };
    let mut mesh = weld(&corners)?;
    mesh.meta = meta;
    Ok(mesh)
}

/// The triangle count of a binary STL, when `bytes` has exactly the length it
/// says.
fn binary_triangle_count(bytes: &[u8]) -> Option<usize> {
    let count = u32::from_le_bytes(bytes.get(80..BINARY_HEADER)?.try_into().ok()?);
    let count = usize::try_from(count).ok()?;
    let length = count
        .checked_mul(BINARY_TRIANGLE)?
        .checked_add(BINARY_HEADER)?;
    (bytes.len() == length).then_some(count)
}

/// The corners of a binary STL of `count` triangles, as the bits of their
/// coordinates: three per triangle, in order.
fn binary_corners(bytes: &[u8], count: usize) -> Vec<[u32; 3]> {
    let mut corners = Vec::with_capacity(3 * count);
    for facet in bytes[BINARY_HEADER..].as_chunks::<BINARY_TRIANGLE>().0 {
        for corner in 0..3 {
            let at = 12 + 12 * corner;
            corners.push(std::array::from_fn(|i| {
                let b = &facet[at + 4 * i..at + 4 * i + 4];
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }));
        }
    }
    corners
}

/// A binary STL's header as the mesh's comment, when it holds text: UTF-8,
/// without control characters, once the NUL and space padding at its end is
/// cut off.
fn binary_meta(bytes: &[u8]) -> Option<Meta> {
    let header = &bytes[..80];
    let end = header
        .iter()
        .rposition(|&b| b != 0 && b != b' ')
        .map_or(0, |i| i + 1);
    let text = std::str::from_utf8(&header[..end]).ok()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(Meta {
        name: None,
        comment: Some(text.to_owned()),
    })
}

/// The corners and solid name of an ASCII STL:
///
/// ```text
/// solid [name]
///   facet normal nx ny nz
///     outer loop
///       vertex x y z     (three times)
///     endloop
///   endfacet             (any number of facets)
/// endsolid [name]
/// ```
///
/// Keywords, in any case, and numbers are separated by any whitespace. Coordinates are read
/// as the nearest `f32`. Anything after `endsolid` is ignored.
fn ascii(bytes: &[u8]) -> Result<(Vec<[u32; 3]>, Option<Meta>), ReadStlError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ReadStlError::NotStl)?;
    let mut lines = text.lines().enumerate().map(|(i, line)| (i + 1, line));
    let name = loop {
        let Some((_, line)) = lines.next() else {
            return Err(ReadStlError::NotStl);
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rest = strip_keyword(line, "solid").ok_or(ReadStlError::NotStl)?;
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Err(ReadStlError::NotStl);
        }
        break rest.trim();
    };

    let mut corners = Vec::new();
    // Where the parser is: 0 between facets, then after `facet`, `outer
    // loop`, each `vertex`, and `endloop`.
    let mut step = 0;
    let mut ended = false;
    for (number, line) in lines {
        let bad = ReadStlError::Ascii { line: number };
        let mut words = line.split_whitespace();
        let Some(word) = words.next() else {
            continue;
        };
        let word = word.to_ascii_lowercase();
        step = match (step, word.as_str()) {
            (0, "facet") => {
                if !is(words.next(), "normal") {
                    return Err(bad);
                }
                for _ in 0..3 {
                    // Read so a damaged normal is caught, but not kept.
                    words.next().and_then(coordinate).ok_or(bad.clone())?;
                }
                1
            }
            (0, "endsolid") => {
                ended = true;
                break;
            }
            (1, "outer") if is(words.next(), "loop") => 2,
            (2..=4, "vertex") => {
                let mut corner = [0; 3];
                for value in &mut corner {
                    *value = words.next().and_then(coordinate).ok_or(bad.clone())?;
                }
                corners.push(corner);
                step + 1
            }
            (5, "endloop") => 6,
            (6, "endfacet") => 0,
            _ => return Err(bad),
        };
        if words.next().is_some() {
            return Err(bad);
        }
    }
    if !ended {
        let last = text.lines().count().max(1);
        return Err(ReadStlError::Ascii { line: last });
    }
    let meta = (!name.is_empty()).then(|| Meta {
        name: Some(name.to_owned()),
        comment: None,
    });
    Ok((corners, meta))
}

/// Whether `word` is `keyword`, in any case.
fn is(word: Option<&str>, keyword: &str) -> bool {
    word.is_some_and(|word| word.eq_ignore_ascii_case(keyword))
}

/// `line` past `keyword` at its start, in any case.
fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let head = line.get(..keyword.len())?;
    head.eq_ignore_ascii_case(keyword)
        .then(|| &line[keyword.len()..])
}

/// A coordinate of an ASCII STL, as the bits of the nearest `f32`.
fn coordinate(word: &str) -> Option<u32> {
    word.parse::<f32>().ok().map(f32::to_bits)
}

/// The corners welded into a mesh: equal bits, one vertex, numbered by first
/// use. `corners` holds three per triangle.
fn weld(corners: &[[u32; 3]]) -> Result<Mesh, ReadStlError> {
    let mut index: HashMap<[u32; 3], u32> = HashMap::with_capacity(corners.len() / 2);
    let mut vertices = Vec::new();
    let mut ids = Vec::with_capacity(corners.len());
    for corner in corners {
        let id = match index.get(corner) {
            Some(&id) => id,
            None => {
                let id = u32::try_from(vertices.len()).map_err(|_| ReadStlError::TooLarge)?;
                vertices.push(corner.map(f32::from_bits));
                index.insert(*corner, id);
                id
            }
        };
        ids.push(id);
    }
    let triangles = ids.as_chunks::<3>().0.to_vec();
    Ok(Mesh {
        meta: None,
        vertices,
        triangles,
    })
}
