/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The scene chunk: the tree of nodes that places the meshes (`SPEC.md`,
//! section 5).
//!
//! A [`Scene`] names everything by index, as the chunk does: meshes in the
//! order of the file's mesh chunks, and nodes in the order they are listed,
//! which is how other chunks name them. A mesh several leaves show is one
//! index they share, stored once.
//!
//! Nodes are listed children first: a node names only nodes listed before it,
//! and a reference only a subtree whose root is, so no node can contain
//! itself. Each node has at most one place: a group's child, a root, or a
//! subtree's root. The decoder refuses a payload that breaks these rules, and
//! the encoder a scene that does, so it never writes one a reader must refuse.
//!
//! ```text
//! metadata    u8 0 | 1, then name and comment (u8 0 | 1, u32 length, UTF-8)
//! meshes      u32 number of mesh chunks
//! resources   u32 count, then u32 mesh per mesh resource
//! records     u32 count, then per record a u8 kind:
//!               0 group      node, u32 count, u32 child nodes
//!               1 leaf       node, u32 mesh
//!               2 leaf       node, u32 mesh resource
//!               4 reference  node, u32 subtree
//!               5 subtree    u32 node, the root of the next subtree
//!             where node is its metadata, then its transform:
//!             u8 0 (identity) | 1 and 12 f64 (rows of the linear part, then
//!             the translation)
//! roots       u32 count, u32 nodes
//! ```

use std::fmt;

use crate::bytes::{In, Out, fits_u32};
use crate::container::ChunkTag;
use crate::mesh::Mesh;
use crate::meta::{Meta, read_meta, write_meta};

/// The tag of the scene chunk.
pub const SCENE_TAG: ChunkTag = ChunkTag(*b"SCNE");

/// The scene payload layout this build writes and reads.
pub const SCENE_VERSION: u16 = 1;

const GROUP: u8 = 0;
const LEAF: u8 = 1;
const SHARED_LEAF: u8 = 2;
const REFERENCE: u8 = 4;
const SUBTREE: u8 = 5;

/// A scene: the meshes, and the nodes that place them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub meta: Option<Meta>,
    /// The meshes, one per mesh chunk, in file order.
    pub meshes: Vec<Mesh>,
    /// The meshes the scene keeps as shared on purpose, as indices into
    /// `meshes`: a program editing one changes every leaf that shows it.
    pub mesh_resources: Vec<usize>,
    /// The nodes, children first.
    pub nodes: Vec<Node>,
    /// The subtrees references copy, as the indices of their root nodes.
    pub subtrees: Vec<usize>,
    /// The top-level nodes, in order.
    pub roots: Vec<usize>,
}

/// A node of the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub meta: Option<Meta>,
    /// The transform from the node's space to its parent's, or `None` for
    /// the identity, which is stored apart from an identity matrix.
    pub transform: Option<Transform>,
    pub kind: NodeKind,
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A group of nodes, as indices into the scene's nodes, in order.
    Group { children: Vec<usize> },
    /// A leaf that shows a mesh, as an index into the scene's meshes.
    Leaf { mesh: usize },
    /// A leaf that shows a shared mesh, as an index into the scene's mesh
    /// resources.
    SharedLeaf { resource: usize },
    /// One more copy of a subtree, as an index into the scene's subtrees.
    Reference { subtree: usize },
}

/// An affine transform: a point *p* goes to `linear` · *p* + `translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// The 3 × 3 matrix, row by row.
    pub linear: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

/// Why a scene could not be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeSceneError {
    /// Something the scene names is missing, or not listed before what
    /// names it, or a node has two places.
    Inconsistent,
    /// The scene has 2^32 or more meshes, mesh resources, roots, or nodes and
    /// subtrees together.
    TooLarge,
}

impl fmt::Display for EncodeSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inconsistent => "the scene graph is inconsistent",
            Self::TooLarge => "the scene is too large for a scene chunk",
        })
    }
}

impl std::error::Error for EncodeSceneError {}

/// Why a scene payload could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeSceneError {
    /// The payload is cut short, padded, names something not listed before
    /// it, or gives a node two places.
    Malformed,
    /// The payload names a different number of meshes than the file holds.
    MeshCount { named: usize, found: usize },
}

impl fmt::Display for DecodeSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("the scene data is malformed"),
            Self::MeshCount { named, found } => {
                write!(
                    f,
                    "the scene names {named} meshes but the file holds {found}"
                )
            }
        }
    }
}

impl std::error::Error for DecodeSceneError {}

/// Encodes `scene` as a scene chunk payload. Its meshes go in mesh chunks of
/// their own, in order.
///
/// Each subtree is listed right after its root, or after the subtree before
/// it when that comes later. Nodes without a place are written as they are,
/// so the indices other chunks use stay valid, but a program building a
/// scene should not leave any.
pub fn encode_scene(scene: &Scene) -> Result<Vec<u8>, EncodeSceneError> {
    let counts = [
        scene.meshes.len(),
        scene.mesh_resources.len(),
        scene.nodes.len() + scene.subtrees.len(),
        scene.roots.len(),
    ];
    if !counts.into_iter().all(fits_u32) {
        return Err(EncodeSceneError::TooLarge);
    }
    encode(scene).ok_or(EncodeSceneError::Inconsistent)
}

fn encode(scene: &Scene) -> Option<Vec<u8>> {
    let mut out = Out::default();
    write_meta(&mut out, scene.meta.as_ref());
    out.len(scene.meshes.len());
    out.len(scene.mesh_resources.len());
    for &mesh in &scene.mesh_resources {
        out.len(below(mesh, scene.meshes.len())?);
    }

    out.len(scene.nodes.len() + scene.subtrees.len());
    let mut placed = vec![false; scene.nodes.len()];
    let mut subtrees = 0;
    for (listed, node) in scene.nodes.iter().enumerate() {
        out.u8(match node.kind {
            NodeKind::Group { .. } => GROUP,
            NodeKind::Leaf { .. } => LEAF,
            NodeKind::SharedLeaf { .. } => SHARED_LEAF,
            NodeKind::Reference { .. } => REFERENCE,
        });
        write_meta(&mut out, node.meta.as_ref());
        write_transform(&mut out, node.transform.as_ref());
        match &node.kind {
            NodeKind::Group { children } => {
                for &child in children {
                    place(&mut placed, child, listed)?;
                }
                out.len(children.len());
                for &child in children {
                    out.len(child);
                }
            }
            NodeKind::Leaf { mesh } => out.len(below(*mesh, scene.meshes.len())?),
            NodeKind::SharedLeaf { resource } => {
                out.len(below(*resource, scene.mesh_resources.len())?);
            }
            NodeKind::Reference { subtree } => out.len(below(*subtree, subtrees)?),
        }
        while let Some(&root) = scene.subtrees.get(subtrees).filter(|&&root| root <= listed) {
            place(&mut placed, root, listed + 1)?;
            out.u8(SUBTREE);
            out.len(root);
            subtrees += 1;
        }
    }
    if subtrees != scene.subtrees.len() {
        return None;
    }

    out.len(scene.roots.len());
    for &root in &scene.roots {
        place(&mut placed, root, scene.nodes.len())?;
        out.len(root);
    }
    Some(out.0)
}

/// Decodes a scene chunk payload over the meshes of the file's mesh chunks,
/// in file order.
pub fn decode_scene(payload: &[u8], meshes: Vec<Mesh>) -> Result<Scene, DecodeSceneError> {
    let mut input = In(payload);
    let meta = read_meta(&mut input).ok_or(DecodeSceneError::Malformed)?;
    let named = input.len().ok_or(DecodeSceneError::Malformed)?;
    if named != meshes.len() {
        return Err(DecodeSceneError::MeshCount {
            named,
            found: meshes.len(),
        });
    }
    let mut scene = Scene {
        meta,
        meshes,
        ..Scene::default()
    };
    decode(&mut input, &mut scene).ok_or(DecodeSceneError::Malformed)?;
    Ok(scene)
}

fn decode(input: &mut In<'_>, scene: &mut Scene) -> Option<()> {
    for _ in 0..input.count(4)? {
        let mesh = below(input.len()?, scene.meshes.len())?;
        scene.mesh_resources.push(mesh);
    }

    let mut placed = Vec::new();
    for _ in 0..input.count(5)? {
        let kind = input.u8()?;
        if kind == SUBTREE {
            let root = input.len()?;
            place(&mut placed, root, scene.nodes.len())?;
            scene.subtrees.push(root);
            continue;
        }
        let meta = read_meta(input)?;
        let transform = read_transform(input)?;
        let kind = match kind {
            GROUP => {
                let count = input.count(4)?;
                let mut children = Vec::with_capacity(count);
                for _ in 0..count {
                    let child = input.len()?;
                    place(&mut placed, child, scene.nodes.len())?;
                    children.push(child);
                }
                NodeKind::Group { children }
            }
            LEAF => NodeKind::Leaf {
                mesh: below(input.len()?, scene.meshes.len())?,
            },
            SHARED_LEAF => NodeKind::SharedLeaf {
                resource: below(input.len()?, scene.mesh_resources.len())?,
            },
            REFERENCE => NodeKind::Reference {
                subtree: below(input.len()?, scene.subtrees.len())?,
            },
            _ => return None,
        };
        scene.nodes.push(Node {
            meta,
            transform,
            kind,
        });
        placed.push(false);
    }

    for _ in 0..input.count(4)? {
        let root = input.len()?;
        place(&mut placed, root, scene.nodes.len())?;
        scene.roots.push(root);
    }
    input.is_empty().then_some(())
}

/// `index`, when it is below `count`.
fn below(index: usize, count: usize) -> Option<usize> {
    (index < count).then_some(index)
}

/// Gives `node` its one place in the scene: `None` when it is not among the
/// first `listed` nodes or already has a place.
fn place(placed: &mut [bool], node: usize, listed: usize) -> Option<()> {
    let slot = placed.get_mut(below(node, listed)?)?;
    (!std::mem::replace(slot, true)).then_some(())
}

fn write_transform(out: &mut Out, transform: Option<&Transform>) {
    match transform {
        None => out.u8(0),
        Some(transform) => {
            out.u8(1);
            for value in transform
                .linear
                .as_flattened()
                .iter()
                .chain(&transform.translation)
            {
                out.f64(*value);
            }
        }
    }
}

fn read_transform(input: &mut In<'_>) -> Option<Option<Transform>> {
    match input.u8()? {
        0 => Some(None),
        1 => {
            let mut transform = Transform {
                linear: [[0.0; 3]; 3],
                translation: [0.0; 3],
            };
            for value in transform
                .linear
                .as_flattened_mut()
                .iter_mut()
                .chain(&mut transform.translation)
            {
                *value = input.f64()?;
            }
            Some(Some(transform))
        }
        _ => None,
    }
}
