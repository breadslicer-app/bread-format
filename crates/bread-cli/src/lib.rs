/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! What the `bread` program does, apart from reading its arguments and its
//! files: STL files made into one `.bread` file and checked corner by corner,
//! a `.bread` file made into one STL, and what a `.bread` file holds.

use std::collections::HashMap;
use std::fmt;

use bread_format::{
    BreadFile, BreadReader, Mesh, Meta, Node, NodeKind, Scene, read_file, read_stl, write_file,
};

/// Why something could not be done, said for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// An STL file to convert: its name, which names its part, and its bytes.
#[derive(Clone, Debug)]
pub struct StlFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A part of a `.bread` file made from STL files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub vertices: usize,
    pub triangles: usize,
}

/// STL files as one `.bread` file, as the viewer converts them: each STL a
/// mesh, shown by a leaf named after its file, in order and with no
/// transform, so the parts stay where the STLs put them. `name`, when given,
/// names the scene.
///
/// The file is checked before it is handed over: read back, every mesh must
/// be the one read from its STL, and every corner of a binary STL the STL's
/// own bytes, all bit for bit.
pub fn from_stl(files: &[StlFile], name: Option<&str>) -> Result<(Vec<u8>, Vec<Part>), Error> {
    if files.is_empty() {
        return Err(Error("no STL file to convert".into()));
    }
    let meshes = files
        .iter()
        .map(|f| read_stl(&f.bytes).map_err(|e| Error(format!("{}: {e}", f.name))))
        .collect::<Result<Vec<_>, _>>()?;
    let named = |name: &str| Meta {
        name: Some(name.to_owned()),
        comment: None,
    };
    let scene = Scene {
        meta: name.map(named),
        nodes: files
            .iter()
            .enumerate()
            .map(|(mesh, f)| Node {
                meta: Some(named(&f.name)),
                transform: None,
                kind: NodeKind::Leaf { mesh },
            })
            .collect(),
        roots: (0..files.len()).collect(),
        meshes,
        mesh_resources: Vec::new(),
        subtrees: Vec::new(),
    };
    let file = BreadFile {
        scene,
        ..BreadFile::default()
    };
    let bytes = write_file(&file, Vec::new())
        .map_err(|e| Error(format!("cannot write the .bread file: {e}")))?;

    let back = read_file(bytes.as_slice())
        .map_err(|e| Error(format!("the .bread file does not read back: {e}")))?;
    let meshes = &file.scene.meshes;
    let same = back.scene.meshes.len() == meshes.len()
        && back
            .scene
            .meshes
            .iter()
            .zip(meshes)
            .all(|(a, b)| same_mesh(a, b))
        && files
            .iter()
            .zip(&back.scene.meshes)
            .all(|(f, m)| same_as_binary_stl(m, &f.bytes) != Some(false));
    if !same {
        return Err(Error(
            "the .bread file does not give back every corner of the STL files".into(),
        ));
    }
    let parts = files
        .iter()
        .zip(meshes)
        .map(|(f, m)| Part {
            name: f.name.clone(),
            vertices: m.vertices.len(),
            triangles: m.triangles.len(),
        })
        .collect();
    Ok((bytes, parts))
}

/// Whether two meshes are the same: name and comment, every vertex bit for
/// bit, and every triangle in order from the same first corner.
fn same_mesh(a: &Mesh, b: &Mesh) -> bool {
    a.meta == b.meta
        && a.triangles == b.triangles
        && a.vertices.len() == b.vertices.len()
        && a.vertices
            .iter()
            .zip(&b.vertices)
            .all(|(p, q)| p.map(f32::to_bits) == q.map(f32::to_bits))
}

/// Bytes before a binary STL's triangles, and bytes per triangle.
const STL_HEADER: usize = 84;
const STL_TRIANGLE: usize = 50;

/// Whether `mesh` gives back the corners of a binary STL, bit for bit and in
/// order; `None` when `stl` is not a binary STL.
fn same_as_binary_stl(mesh: &Mesh, stl: &[u8]) -> Option<bool> {
    let count = u32::from_le_bytes(stl.get(80..84)?.try_into().ok()?) as usize;
    if stl.len() != STL_HEADER + STL_TRIANGLE * count {
        return None;
    }
    if mesh.triangles.len() != count {
        return Some(false);
    }
    for (t, triangle) in mesh.triangles.iter().enumerate() {
        let corners = STL_HEADER + STL_TRIANGLE * t + 12;
        for (c, &v) in triangle.iter().enumerate() {
            let Some(vertex) = mesh.vertices.get(v as usize) else {
                return Some(false);
            };
            for (k, x) in vertex.iter().enumerate() {
                let at = corners + 12 * c + 4 * k;
                if stl[at..at + 4] != x.to_bits().to_le_bytes() {
                    return Some(false);
                }
            }
        }
    }
    Some(true)
}

/// How many nodes [`place`] walks at most, each copy counted, as the
/// JavaScript package's `placeMeshes`.
pub const PLACEMENT_LIMIT: usize = 1_000_000;

/// A leaf shown, and its world transform as three rows of four, or `None`
/// for the identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub leaf: usize,
    pub world: Option<[f64; 12]>,
}

/// Every leaf the scene shows and its world transform, in the order of the
/// walk of SPEC.md 5.4: down from each root in order, each node's transform
/// after its parent's, and a reference going on into its subtree's root.
///
/// A small file can ask for a great many copies of copies: past
/// [`PLACEMENT_LIMIT`] nodes walked, this gives up.
pub fn place(scene: &Scene) -> Result<Vec<Placed>, Error> {
    let malformed = || Error("the scene names a node it does not have".into());
    let mut placed = Vec::new();
    let mut stack: Vec<(usize, Option<[f64; 12]>)> =
        scene.roots.iter().rev().map(|&root| (root, None)).collect();
    let mut walked = 0;
    while let Some((index, parent)) = stack.pop() {
        walked += 1;
        if walked > PLACEMENT_LIMIT {
            return Err(Error(format!(
                "the scene shows more than {PLACEMENT_LIMIT} nodes"
            )));
        }
        let node = scene.nodes.get(index).ok_or_else(malformed)?;
        let world = match (parent, node.transform) {
            (parent, None) => parent,
            (parent, Some(t)) => {
                let [r0, r1, r2] = t.linear;
                let [x, y, z] = t.translation;
                let own = [
                    r0[0], r0[1], r0[2], x, r1[0], r1[1], r1[2], y, r2[0], r2[1], r2[2], z,
                ];
                Some(match parent {
                    None => own,
                    Some(p) => compose(&p, &own),
                })
            }
        };
        match &node.kind {
            NodeKind::Group { children } => {
                stack.extend(children.iter().rev().map(|&child| (child, world)));
            }
            NodeKind::Reference { subtree } => {
                let root = *scene.subtrees.get(*subtree).ok_or_else(malformed)?;
                stack.push((root, world));
            }
            NodeKind::Leaf { .. } | NodeKind::SharedLeaf { .. } => {
                placed.push(Placed { leaf: index, world });
            }
        }
    }
    Ok(placed)
}

/// `a` after `b`: a point p goes to a(b(p)).
fn compose(a: &[f64; 12], b: &[f64; 12]) -> [f64; 12] {
    let mut out = [0.0; 12];
    for r in 0..3 {
        for c in 0..4 {
            let mut sum = a[4 * r] * b[c] + a[4 * r + 1] * b[4 + c] + a[4 * r + 2] * b[8 + c];
            if c == 3 {
                sum += a[4 * r + 3];
            }
            out[4 * r + c] = sum;
        }
    }
    out
}

/// The header of the STL files [`to_stl`] writes.
const STL_HEADER_TEXT: &[u8] = b"binary STL written from a .bread file";

/// A `.bread` file as one binary STL of everything it shows, as the viewer's
/// "Download as STL": each part in its world position, each vertex moved by
/// its leaf's transform in f64 and then rounded to f32. Parts with no
/// transform keep their vertices bit for bit. A support leaf shows its
/// support preview, when the file has one. Facet normals are written as
/// zero, which programs read as "work it out".
pub fn to_stl(file: &BreadFile) -> Result<(Vec<u8>, usize), Error> {
    let previews: HashMap<usize, &Mesh> = file
        .support_previews
        .iter()
        .map(|p| (p.leaf, &p.mesh))
        .collect();
    let parts: Vec<(&Mesh, Option<[f64; 12]>)> = place(&file.scene)?
        .into_iter()
        .filter_map(|p| {
            let mesh = previews
                .get(&p.leaf)
                .copied()
                .or_else(|| file.scene.leaf_mesh(p.leaf))?;
            (!mesh.triangles.is_empty()).then_some((mesh, p.world))
        })
        .collect();
    let count: usize = parts.iter().map(|(m, _)| m.triangles.len()).sum();
    let too_many = || Error("the file shows more triangles than an STL can hold".into());
    let count32 = u32::try_from(count).map_err(|_| too_many())?;
    let size = STL_TRIANGLE
        .checked_mul(count)
        .and_then(|n| n.checked_add(STL_HEADER))
        .ok_or_else(too_many)?;

    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(STL_HEADER_TEXT);
    out.resize(80, 0);
    out.extend_from_slice(&count32.to_le_bytes());
    for (mesh, world) in &parts {
        let moved: Vec<[f32; 3]>;
        let vertices = match world {
            None => &mesh.vertices,
            Some(m) => {
                moved = mesh.vertices.iter().map(|v| apply(m, v)).collect();
                &moved
            }
        };
        // A transform that mirrors turns the faces inside out: keep them
        // facing out by swapping two corners.
        let order = match world {
            Some(m) if determinant(m) < 0.0 => [0, 2, 1],
            _ => [0, 1, 2],
        };
        for triangle in &mesh.triangles {
            out.extend_from_slice(&[0; 12]);
            for c in order {
                let v = vertices.get(triangle[c] as usize).ok_or_else(|| {
                    Error("a triangle names a vertex its mesh does not have".into())
                })?;
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            out.extend_from_slice(&[0; 2]);
        }
    }
    Ok((out, parts.len()))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "f64 to f32, as SPEC.md 5.4 asks"
)]
fn apply(m: &[f64; 12], v: &[f32; 3]) -> [f32; 3] {
    let [x, y, z] = v.map(f64::from);
    [0, 1, 2].map(|r| (m[4 * r] * x + m[4 * r + 1] * y + m[4 * r + 2] * z + m[4 * r + 3]) as f32)
}

fn determinant(m: &[f64; 12]) -> f64 {
    m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8])
}

/// A count with its thousands apart: 1,234,567.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}

/// What a `.bread` file holds, said in lines: its chunks, its meshes, the
/// parts it shows and the slicer data it keeps. A file the format refuses
/// gives the reason instead.
pub fn info(bytes: &[u8]) -> Result<Vec<String>, Error> {
    let mut lines = vec![format!("{} bytes", grouped(bytes.len()))];
    let mut reader = BreadReader::new(bytes).map_err(|e| Error(e.to_string()))?;
    lines.push("chunks (tag, version, bytes once decompressed):".into());
    while let Some(chunk) = reader.next_chunk().map_err(|e| Error(e.to_string()))? {
        lines.push(format!(
            "  {} v{}  {}",
            chunk.tag,
            chunk.version,
            grouped(chunk.payload.len())
        ));
    }
    let file = read_file(bytes).map_err(|e| Error(e.to_string()))?;
    let scene = &file.scene;
    if let Some(name) = scene.meta.as_ref().and_then(|m| m.name.as_deref()) {
        lines.push(format!("scene: {name}"));
    }
    let triangles: usize = scene.meshes.iter().map(|m| m.triangles.len()).sum();
    lines.push(format!(
        "{} mesh{}, {} triangles:",
        scene.meshes.len(),
        if scene.meshes.len() == 1 { "" } else { "es" },
        grouped(triangles)
    ));
    for (i, mesh) in scene.meshes.iter().enumerate() {
        let name = mesh.meta.as_ref().and_then(|m| m.name.as_deref());
        lines.push(format!(
            "  {i}{}  {} vertices, {} triangles",
            name.map(|n| format!(" {n}")).unwrap_or_default(),
            grouped(mesh.vertices.len()),
            grouped(mesh.triangles.len())
        ));
    }
    let placed = place(scene)?;
    lines.push(format!(
        "{} part{} shown, {} node{}",
        grouped(placed.len()),
        if placed.len() == 1 { "" } else { "s" },
        grouped(scene.nodes.len()),
        if scene.nodes.len() == 1 { "" } else { "s" },
    ));
    let mut extra = Vec::new();
    if !file.supports.is_empty() {
        extra.push(format!("{} set(s) of supports", file.supports.len()));
    }
    if !file.support_previews.is_empty() {
        extra.push(format!(
            "{} support preview(s)",
            file.support_previews.len()
        ));
    }
    if !file.hollows.is_empty() {
        extra.push(format!("{} hollowed model(s)", file.hollows.len()));
    }
    if !file.surface.is_empty() {
        extra.push(format!("{} painted surface(s)", file.surface.len()));
    }
    if let Some(settings) = &file.settings {
        extra.push(format!("settings ({} bytes)", grouped(settings.len())));
    }
    if let Some(thumbnail) = &file.thumbnail {
        extra.push(format!("a thumbnail ({} bytes)", grouped(thumbnail.len())));
    }
    if !extra.is_empty() {
        lines.push(format!("also: {}", extra.join(", ")));
    }
    Ok(lines)
}

/// Reads a `.bread` file as a reader of the format must, and walks its
/// scene: what it holds when it reads, or why it is refused.
pub fn check(bytes: &[u8]) -> Result<String, Error> {
    let file = read_file(bytes).map_err(|e| Error(e.to_string()))?;
    let placed = place(&file.scene)?;
    let triangles: usize = file.scene.meshes.iter().map(|m| m.triangles.len()).sum();
    Ok(format!(
        "{} mesh{}, {} triangles, {} part{} shown",
        file.scene.meshes.len(),
        if file.scene.meshes.len() == 1 {
            ""
        } else {
            "es"
        },
        grouped(triangles),
        grouped(placed.len()),
        if placed.len() == 1 { "" } else { "s" },
    ))
}

#[cfg(test)]
mod tests;
