/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The conformance files in `conformance/` at the root of the repository
//! (see the README there): in `valid/`, small `.bread` files every reader must
//! open, each with a JSON file beside it that describes what it holds; in
//! `invalid/`, files every reader must refuse, each with a JSON file that
//! gives the reason.
//!
//! The cases below make the files. `the_files_match_their_cases` checks that
//! writing each valid case still gives the stored bytes, so a change to them
//! is seen, and that reading the stored file gives its JSON;
//! `the_invalid_files_match_their_cases` checks the same of the invalid cases,
//! and that the crate refuses each file for its reason. After a change on
//! purpose, the files are made again with:
//!
//! ```text
//! cargo test -p bread-format --test conformance -- --ignored
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bread_format::{
    BreadFile, BreadReader, BreadWriter, ChunkTag, Connectivity, DecodeMeshError, DecodeSceneError,
    HollowEntry, MESH_TAG, Mesh, MeshCoding, Meta, Node, NodeKind, ReadBreadError, ReadFileError,
    SCENE_TAG, SUPPORT_PREVIEW_TAG, Scene, SupportEntry, SupportPreview, SurfaceEntry,
    THUMBNAIL_TAG, Transform, VertexCoding, encode_mesh_v2_with, read_file, write_file,
};
use sha2::{Digest, Sha256, Sha384};

/// A conformance file: its name, what it shows, and what it holds.
struct Case {
    name: &'static str,
    description: &'static str,
    file: fn() -> BreadFile,
    /// Chunks a reader skips, written right after the scene chunk.
    skipped: &'static [Extra],
}

/// A chunk the crate does not write: its tag, version and payload.
type Extra = (ChunkTag, u16, &'static [u8]);

const CASES: &[Case] = &[
    Case {
        name: "triangle",
        description: "One triangle, shown by one leaf, with no names and no transform.",
        file: triangle,
        skipped: &[],
    },
    Case {
        name: "cube",
        description: "A unit cube of 8 vertices and 12 triangles, with a named scene, node and \
                      mesh.",
        file: cube,
        skipped: &[],
    },
    Case {
        name: "two-parts",
        description: "A named group of two named leaves, each showing its own mesh, one of them \
                      moved. One name holds text beyond ASCII, and the group has a comment.",
        file: two_parts,
        skipped: &[],
    },
    Case {
        name: "shared-mesh",
        description: "One mesh, kept as a mesh resource and shown by two leaves with different \
                      transforms: one moved, one turned, scaled and moved.",
        file: shared_mesh,
        skipped: &[],
    },
    Case {
        name: "rotated-triangle",
        description: "One triangle listed from its second corner, which the triangle codec \
                      stores from another corner: a reader gives back the listed one.",
        file: rotated_triangle,
        skipped: &[],
    },
    Case {
        name: "unknown-chunk",
        description: "The triangle file with a chunk of an official-looking tag that no reader \
                      knows, at version 9, after the scene chunk: a reader skips it.",
        file: triangle,
        skipped: &[(ChunkTag(*b"ZZZZ"), 9, b"a chunk no reader knows")],
    },
    Case {
        name: "private-chunk",
        description: "The triangle file with a private chunk after the scene chunk: a reader \
                      that does not know it skips it.",
        file: triangle,
        skipped: &[(ChunkTag(*b"xmpl"), 1, b"example: private data")],
    },
    Case {
        name: "slicer-data",
        description: "A hollowed model with supports: project settings, a supports entry whose \
                      support leaf shows an empty mesh, and a hollows entry whose original mesh \
                      no leaf shows.",
        file: slicer_data,
        skipped: &[],
    },
    Case {
        name: "thumbnail",
        description: "The triangle file with a thumbnail, written as the first chunk: a PNG of \
                      2 x 2 pixels, three of them opaque and one transparent.",
        file: thumbnail,
        skipped: &[],
    },
    Case {
        name: "support-preview",
        description: "A triangle model with supports whose support leaf shows an empty mesh, \
                      and a preview of those supports: a reader that shows the preview shows \
                      a post under the triangle.",
        file: support_preview,
        skipped: &[],
    },
    Case {
        name: "surface-paint",
        description: "The cube with paint on its faces: detail on its first two triangles, and \
                      supports kept off two triangles at half strength. Each mask names the cube \
                      by its fingerprint.",
        file: surface_paint,
        skipped: &[],
    },
];

/// A PNG of 2 x 2 pixels, 8-bit RGBA: crust brown and crumb on top, crumb
/// and a transparent pixel below.
const THUMBNAIL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x17, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xe8, 0x8e, 0xd2, 0xfe,
    0xff, 0xe9, 0xda, 0xec, 0xff, 0x0c, 0x60, 0x82, 0x81, 0x81, 0x01, 0x00, 0x5c, 0x5c, 0x08, 0xd4,
    0x41, 0xf2, 0x7c, 0x42, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn named(name: &str) -> Option<Meta> {
    Some(Meta {
        name: Some(name.into()),
        comment: None,
    })
}

fn leaf(meta: Option<Meta>, transform: Option<Transform>, mesh: usize) -> Node {
    Node {
        meta,
        transform,
        kind: NodeKind::Leaf { mesh },
    }
}

fn moved(translation: [f64; 3]) -> Option<Transform> {
    Some(Transform {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation,
    })
}

fn triangle_mesh(triangle: [u32; 3]) -> Mesh {
    Mesh {
        meta: None,
        vertices: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        triangles: vec![triangle],
    }
}

/// A scene of one leaf, at the root, showing `mesh`.
fn one_leaf(scene: Option<Meta>, node: Option<Meta>, mesh: Mesh) -> BreadFile {
    BreadFile {
        scene: Scene {
            meta: scene,
            meshes: vec![mesh],
            nodes: vec![leaf(node, None, 0)],
            roots: vec![0],
            ..Scene::default()
        },
        ..BreadFile::default()
    }
}

fn triangle() -> BreadFile {
    one_leaf(None, None, triangle_mesh([0, 1, 2]))
}

fn thumbnail() -> BreadFile {
    BreadFile {
        thumbnail: Some(THUMBNAIL_PNG.to_vec()),
        ..triangle()
    }
}

fn rotated_triangle() -> BreadFile {
    one_leaf(None, None, triangle_mesh([1, 2, 0]))
}

fn cube() -> BreadFile {
    one_leaf(named("cube scene"), named("cube"), cube_mesh("cube mesh"))
}

/// The SHA-384 fingerprint of `mesh` (`SPEC.md`, section 10.1).
fn fingerprint(mesh: &Mesh) -> [u8; 48] {
    let mut hasher = Sha384::new();
    hasher.update((mesh.vertices.len() as u64).to_le_bytes());
    for vertex in &mesh.vertices {
        for coordinate in vertex {
            hasher.update(coordinate.to_bits().to_le_bytes());
        }
    }
    hasher.update((mesh.triangles.len() as u64).to_le_bytes());
    for triangle in &mesh.triangles {
        for index in triangle {
            hasher.update(index.to_le_bytes());
        }
    }
    hasher.finalize().into()
}

/// A version 1 paint document for `mesh`, from its runs.
fn mask(mesh: &Mesh, runs: &[(u32, u8)]) -> Vec<u8> {
    let mut document = (mesh.triangles.len() as u32).to_le_bytes().to_vec();
    document.extend(fingerprint(mesh));
    document.extend((runs.len() as u32).to_le_bytes());
    for &(length, value) in runs {
        document.extend(length.to_le_bytes());
        document.push(value);
    }
    document
}

fn surface_paint() -> BreadFile {
    let file = cube();
    let mesh = &file.scene.meshes[0];
    let surface = vec![
        SurfaceEntry {
            leaf: 0,
            kind: 0,
            version: 1,
            document: mask(mesh, &[(2, 255), (10, 0)]),
        },
        SurfaceEntry {
            leaf: 0,
            kind: 2,
            version: 1,
            document: mask(mesh, &[(4, 0), (2, 128), (6, 0)]),
        },
    ];
    BreadFile { surface, ..file }
}

fn two_parts() -> BreadFile {
    let group = Node {
        meta: Some(Meta {
            name: Some("assembly".into()),
            comment: Some("Two parts that print together.".into()),
        }),
        transform: None,
        kind: NodeKind::Group {
            children: vec![0, 1],
        },
    };
    BreadFile {
        scene: Scene {
            meshes: vec![cube_mesh("base"), triangle_mesh([0, 1, 2])],
            nodes: vec![
                leaf(named("base"), None, 0),
                leaf(named("lid \u{2014} top"), moved([0.0, 0.0, 1.5]), 1),
                group,
            ],
            roots: vec![2],
            ..Scene::default()
        },
        ..BreadFile::default()
    }
}

fn shared_mesh() -> BreadFile {
    let shared = |name, transform| Node {
        meta: named(name),
        transform,
        kind: NodeKind::SharedLeaf { resource: 0 },
    };
    // A quarter turn about z at half size, then moved.
    let turned = Transform {
        linear: [[0.0, -0.5, 0.0], [0.5, 0.0, 0.0], [0.0, 0.0, 0.5]],
        translation: [-3.25, 0.1, 1e-3],
    };
    BreadFile {
        scene: Scene {
            meshes: vec![cube_mesh("shared cube")],
            mesh_resources: vec![0],
            nodes: vec![
                shared("left", moved([2.0, 0.0, 0.0])),
                shared("right", Some(turned)),
            ],
            roots: vec![0, 1],
            ..Scene::default()
        },
        ..BreadFile::default()
    }
}

fn slicer_data() -> BreadFile {
    let empty = Mesh {
        meta: None,
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    BreadFile {
        scene: Scene {
            meshes: vec![triangle_mesh([0, 1, 2]), empty, cube_mesh("original")],
            nodes: vec![
                leaf(named("model"), None, 0),
                leaf(named("supports"), None, 1),
            ],
            roots: vec![0, 1],
            ..Scene::default()
        },
        settings: Some(b"version = 1\n".to_vec()),
        supports: vec![SupportEntry {
            model: 0,
            support: 1,
            generator: "example".into(),
            version: 1,
            document: b"supports document".to_vec(),
        }],
        hollows: vec![HollowEntry {
            model: 0,
            original: 2,
            generator: "example".into(),
            version: 1,
            document: b"hollows document".to_vec(),
        }],
        thumbnail: None,
        support_previews: Vec::new(),
        surface: Vec::new(),
    }
}

/// A model on supports: the triangle, raised 5 above the plate, and its
/// supports, a post kept as data whose preview reaches up to the triangle.
fn support_preview() -> BreadFile {
    let empty = Mesh {
        meta: None,
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    // A thin, three-sided post from the plate up to the model.
    let post = Mesh {
        meta: named("post"),
        vertices: vec![
            [1.0, 1.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, 2.0, 0.0],
            [1.0, 1.0, 5.0],
            [2.0, 1.0, 5.0],
            [1.0, 2.0, 5.0],
        ],
        triangles: vec![
            [0, 2, 1],
            [3, 4, 5],
            [0, 1, 4],
            [0, 4, 3],
            [1, 2, 5],
            [1, 5, 4],
            [2, 0, 3],
            [2, 3, 5],
        ],
    };
    BreadFile {
        scene: Scene {
            meshes: vec![triangle_mesh([0, 1, 2]), empty],
            nodes: vec![
                leaf(named("model"), moved([0.0, 0.0, 5.0]), 0),
                leaf(named("supports"), None, 1),
            ],
            roots: vec![0, 1],
            ..Scene::default()
        },
        supports: vec![SupportEntry {
            model: 0,
            support: 1,
            generator: "example".into(),
            version: 1,
            document: b"supports document".to_vec(),
        }],
        support_previews: vec![SupportPreview {
            leaf: 1,
            mesh: post,
        }],
        ..BreadFile::default()
    }
}

fn cube_mesh(name: &str) -> Mesh {
    let vertices = (0..8)
        .map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|c| c as f32))
        .collect();
    // Two triangles per face, wound counterclockwise seen from outside.
    let triangles = vec![
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    Mesh {
        meta: named(name),
        vertices,
        triangles,
    }
}

/// A JSON value, written by hand: the files need only a few kinds of field.
enum Json {
    Null,
    Number(String),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    fn int(n: impl Into<u64>) -> Self {
        Self::Number(n.into().to_string())
    }

    fn index(n: usize) -> Self {
        Self::Number(n.to_string())
    }

    /// The shortest decimal that reads back as the same `f64`.
    fn float(x: f64) -> Self {
        assert!(x.is_finite(), "JSON has no number for {x}");
        Self::Number(format!("{x:?}"))
    }

    fn text(s: &str) -> Self {
        Self::Text(s.into())
    }

    fn or_null<T>(value: Option<T>, f: impl FnOnce(T) -> Self) -> Self {
        value.map_or(Self::Null, f)
    }

    fn indices(list: &[usize]) -> Self {
        Self::List(list.iter().copied().map(Self::index).collect())
    }

    /// A list holding only numbers, text and nulls goes on one line.
    fn is_flat(&self) -> bool {
        let scalar = |v: &Self| matches!(v, Self::Null | Self::Number(_) | Self::Text(_));
        match self {
            Self::List(items) => items.iter().all(scalar),
            _ => false,
        }
    }

    fn write(&self, out: &mut String, indent: usize) {
        let pad = |out: &mut String, depth: usize| out.push_str(&"  ".repeat(depth));
        match self {
            Self::Null => out.push_str("null"),
            Self::Number(n) => out.push_str(n),
            Self::Text(s) => write_text(out, s),
            Self::List(items) if items.is_empty() => out.push_str("[]"),
            Self::List(items) if self.is_flat() => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write(out, indent);
                }
                out.push(']');
            }
            Self::List(items) => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    pad(out, indent + 1);
                    item.write(out, indent + 1);
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                pad(out, indent);
                out.push(']');
            }
            Self::Object(fields) => {
                out.push_str("{\n");
                for (i, (key, value)) in fields.iter().enumerate() {
                    pad(out, indent + 1);
                    write_text(out, key);
                    out.push_str(": ");
                    value.write(out, indent + 1);
                    out.push_str(if i + 1 < fields.len() { ",\n" } else { "\n" });
                }
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn write_text(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn sha256(bytes: impl IntoIterator<Item = u8>) -> Json {
    let mut hasher = Sha256::new();
    for byte in bytes {
        hasher.update([byte]);
    }
    let mut hex = String::new();
    for byte in hasher.finalize() {
        write!(hex, "{byte:02x}").unwrap();
    }
    Json::Text(hex)
}

fn meta(meta: Option<&Meta>) -> Json {
    Json::or_null(meta, |meta| {
        Json::Object(vec![
            ("name", Json::or_null(meta.name.as_deref(), Json::text)),
            (
                "comment",
                Json::or_null(meta.comment.as_deref(), Json::text),
            ),
        ])
    })
}

fn mesh(mesh: &Mesh) -> Json {
    let vertex_bits = mesh
        .vertices
        .iter()
        .flatten()
        .flat_map(|c| c.to_bits().to_le_bytes());
    let index_bytes = mesh
        .triangles
        .iter()
        .flatten()
        .flat_map(|i| i.to_le_bytes());
    Json::Object(vec![
        ("meta", meta(mesh.meta.as_ref())),
        ("vertex_count", Json::index(mesh.vertices.len())),
        ("triangle_count", Json::index(mesh.triangles.len())),
        ("vertices_sha256", sha256(vertex_bits)),
        ("triangles_sha256", sha256(index_bytes)),
    ])
}

fn transform(transform: &Transform) -> Json {
    let row = |row: &[f64; 3]| Json::List(row.iter().copied().map(Json::float).collect());
    Json::Object(vec![
        (
            "linear",
            Json::List(transform.linear.iter().map(row).collect()),
        ),
        ("translation", row(&transform.translation)),
    ])
}

fn node(node: &Node) -> Json {
    let (kind, field, value) = match &node.kind {
        NodeKind::Group { children } => ("group", "children", Json::indices(children)),
        NodeKind::Leaf { mesh } => ("leaf", "mesh", Json::index(*mesh)),
        NodeKind::SharedLeaf { resource } => ("shared leaf", "resource", Json::index(*resource)),
        NodeKind::Reference { subtree } => ("reference", "subtree", Json::index(*subtree)),
    };
    Json::Object(vec![
        ("kind", Json::text(kind)),
        ("meta", meta(node.meta.as_ref())),
        (
            "transform",
            Json::or_null(node.transform.as_ref(), transform),
        ),
        (field, value),
    ])
}

fn document(
    mut fields: Vec<(&'static str, Json)>,
    generator: &str,
    version: u32,
    doc: &[u8],
) -> Json {
    fields.extend([
        ("generator", Json::text(generator)),
        ("version", Json::int(version)),
        ("document_size", Json::index(doc.len())),
        ("document_sha256", sha256(doc.iter().copied())),
    ]);
    Json::Object(fields)
}

/// A payload carried without being read: `null`, or its size and SHA-256.
fn payload(payload: Option<&[u8]>) -> Json {
    Json::or_null(payload, |payload| {
        Json::Object(vec![
            ("size", Json::index(payload.len())),
            ("sha256", sha256(payload.iter().copied())),
        ])
    })
}

/// The JSON that describes `file`.
fn describe(description: &str, file: &BreadFile) -> String {
    let scene = &file.scene;
    let settings = payload(file.settings.as_deref());
    let supports = file.supports.iter().map(|entry| {
        let fields = vec![
            ("model", Json::index(entry.model)),
            ("support", Json::index(entry.support)),
        ];
        document(fields, &entry.generator, entry.version, &entry.document)
    });
    let hollows = file.hollows.iter().map(|entry| {
        let fields = vec![
            ("model", Json::index(entry.model)),
            ("original", Json::index(entry.original)),
        ];
        document(fields, &entry.generator, entry.version, &entry.document)
    });
    let previews = file.support_previews.iter().map(|preview| {
        Json::Object(vec![
            ("leaf", Json::index(preview.leaf)),
            ("mesh", mesh(&preview.mesh)),
        ])
    });
    let surface = file.surface.iter().map(|entry| {
        Json::Object(vec![
            ("leaf", Json::index(entry.leaf)),
            ("kind", Json::int(u32::from(entry.kind))),
            ("version", Json::int(entry.version)),
            ("document_size", Json::index(entry.document.len())),
            ("document_sha256", sha256(entry.document.iter().copied())),
        ])
    });
    let json = Json::Object(vec![
        ("description", Json::text(description)),
        ("meta", meta(scene.meta.as_ref())),
        (
            "meshes",
            Json::List(scene.meshes.iter().map(mesh).collect()),
        ),
        ("mesh_resources", Json::indices(&scene.mesh_resources)),
        ("nodes", Json::List(scene.nodes.iter().map(node).collect())),
        ("subtrees", Json::indices(&scene.subtrees)),
        ("roots", Json::indices(&scene.roots)),
        ("settings", settings),
        ("supports", Json::List(supports.collect())),
        ("hollows", Json::List(hollows.collect())),
        ("thumbnail", payload(file.thumbnail.as_deref())),
        ("support_previews", Json::List(previews.collect())),
        ("surface", Json::List(surface.collect())),
    ]);
    let mut out = String::new();
    json.write(&mut out, 0);
    out.push('\n');
    out
}

fn valid_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/valid")
}

/// `bytes` with the `skipped` chunks written right after the scene chunk.
fn with_skipped(bytes: Vec<u8>, skipped: &[Extra]) -> Vec<u8> {
    if skipped.is_empty() {
        return bytes;
    }
    let mut reader = BreadReader::new(bytes.as_slice()).expect("the header reads");
    let mut writer = BreadWriter::new(Vec::new()).expect("the header writes");
    let mut first = true;
    while let Some(chunk) = reader.next_chunk().expect("the chunk reads") {
        writer
            .chunk(chunk.tag, chunk.version, &chunk.payload)
            .expect("the chunk writes");
        if std::mem::take(&mut first) {
            for &(tag, version, payload) in skipped {
                writer
                    .chunk(tag, version, payload)
                    .expect("the chunk writes");
            }
        }
    }
    writer.finish().expect("the file ends")
}

/// The bytes of a case and the JSON that describes them.
fn generate(case: &Case) -> (Vec<u8>, String) {
    let file = (case.file)();
    let bytes = write_file(&file, Vec::new()).expect("the case writes");
    let bytes = with_skipped(bytes, case.skipped);
    let back = read_file(bytes.as_slice()).expect("the case reads");
    assert!(back == file, "{} does not come back the same", case.name);
    let json = describe(case.description, &back);
    (bytes, json)
}

#[test]
fn the_files_match_their_cases() {
    let dir = valid_dir();
    for case in CASES {
        let (bytes, _) = generate(case);
        let stored = std::fs::read(dir.join(format!("{}.bread", case.name)))
            .unwrap_or_else(|err| panic!("{}.bread: {err}", case.name));
        assert!(
            stored == bytes,
            "{}.bread is not what the case writes",
            case.name
        );

        let file = read_file(stored.as_slice()).expect("the stored file reads");
        let json = std::fs::read_to_string(dir.join(format!("{}.json", case.name)))
            .unwrap_or_else(|err| panic!("{}.json: {err}", case.name));
        assert_eq!(
            json,
            describe(case.description, &file),
            "{}.json",
            case.name
        );
    }
}

/// The rotated triangle only means something if the codec stores it from
/// another corner. Its mesh chunk then lists one rotation: the payload ends
/// with a count of 1, a gap and a corner.
#[test]
fn the_rotated_triangle_is_stored_rotated() {
    let case = CASES.iter().find(|case| case.name == "rotated-triangle");
    let file = (case.expect("the case exists").file)();
    let bytes = write_file(&file, Vec::new()).expect("the case writes");
    let mut reader = BreadReader::new(bytes.as_slice()).expect("the header reads");
    while let Some(chunk) = reader.next_chunk().expect("the chunk reads") {
        if chunk.tag == MESH_TAG {
            let parts = mesh_parts(&chunk.payload);
            assert_eq!(parts[1], [1], "the codec rotates the triangle");
            return;
        }
    }
    panic!("the file has no mesh chunk");
}

/// Section 8.1 asks for the thumbnail as the first chunk, so a program that
/// only wants the picture reads one chunk.
#[test]
fn the_thumbnail_is_the_first_chunk() {
    let bytes = std::fs::read(valid_dir().join("thumbnail.bread")).expect("the file exists");
    let mut reader = BreadReader::new(bytes.as_slice()).expect("the header reads");
    let first = reader.next_chunk().expect("the chunk reads");
    let first = first.expect("the file has a chunk");
    assert_eq!(first.tag, THUMBNAIL_TAG);
    assert_eq!(first.payload, THUMBNAIL_PNG);
}

/// The support preview comes after the mesh chunks, where the crate writes
/// it, and holds one preview.
#[test]
fn the_support_preview_is_the_last_chunk() {
    let bytes = std::fs::read(valid_dir().join("support-preview.bread")).expect("the file exists");
    let mut reader = BreadReader::new(bytes.as_slice()).expect("the header reads");
    let mut last = None;
    while let Some(chunk) = reader.next_chunk().expect("the chunk reads") {
        last = Some(chunk);
    }
    let last = last.expect("the file has chunks");
    assert_eq!(last.tag, SUPPORT_PREVIEW_TAG);
    assert_eq!(last.payload[..4], 1u32.to_le_bytes());
}

/// A file every reader must refuse: its name, what is wrong with it, the
/// reason a reader gives, and the chunk the reason is about, if any.
struct Invalid {
    name: &'static str,
    description: &'static str,
    reason: &'static str,
    chunk: Option<ChunkTag>,
    bytes: fn() -> Vec<u8>,
}

const INVALID: &[Invalid] = &[
    Invalid {
        name: "wrong-magic",
        description: "The triangle file with its magic spelled `bread` instead of `BREAD`.",
        reason: "not-bread",
        chunk: None,
        bytes: wrong_magic,
    },
    Invalid {
        name: "text-mode-magic",
        description: "The triangle file after a text-mode transfer turned every CR LF into LF, \
                      which changes the magic.",
        reason: "not-bread",
        chunk: None,
        bytes: text_mode_magic,
    },
    Invalid {
        name: "newer-container",
        description: "The triangle file with container version 2.",
        reason: "version",
        chunk: None,
        bytes: newer_container,
    },
    Invalid {
        name: "cut-short",
        description: "The triangle file with its last byte cut off, inside the stored bytes of \
                      the mesh chunk.",
        reason: "cut-short",
        chunk: None,
        bytes: cut_short,
    },
    Invalid {
        name: "crc-mismatch",
        description: "The triangle file with one bit flipped in the stored bytes of the mesh \
                      chunk, so they no longer match its CRC.",
        reason: "crc",
        chunk: Some(MESH_TAG),
        bytes: crc_mismatch,
    },
    Invalid {
        name: "wrong-raw-length",
        description: "The triangle file with the raw length of the scene chunk one byte more \
                      than its payload decompresses to.",
        reason: "bad-payload",
        chunk: Some(SCENE_TAG),
        bytes: wrong_raw_length,
    },
    Invalid {
        name: "huge-raw-length",
        description: "The triangle file with the raw length of the mesh chunk set to 2^63: a \
                      reader that allocates it up front asks for 8 EiB.",
        reason: "bad-payload",
        chunk: Some(MESH_TAG),
        bytes: huge_raw_length,
    },
    Invalid {
        name: "newer-chunk",
        description: "The triangle file with its mesh chunk at version 3, which this version of \
                      the format does not describe.",
        reason: "version",
        chunk: Some(MESH_TAG),
        bytes: newer_chunk,
    },
    Invalid {
        name: "no-such-vertex",
        description: "A mesh of 3 vertices whose triangle names vertex 3, which it does not have.",
        reason: "no-such-vertex",
        chunk: Some(MESH_TAG),
        bytes: no_such_vertex,
    },
    Invalid {
        name: "no-such-mesh",
        description: "A scene of one mesh whose leaf names mesh 1, which does not exist.",
        reason: "malformed",
        chunk: Some(SCENE_TAG),
        bytes: no_such_mesh,
    },
    Invalid {
        name: "two-scenes",
        description: "The triangle file with its scene chunk written twice.",
        reason: "chunk-count",
        chunk: Some(SCENE_TAG),
        bytes: two_scenes,
    },
    Invalid {
        name: "huge-vertex-count",
        description: "The triangle file with the vertex count of its mesh set to 2^32 - 1, far \
                      more than its vertex buffer can hold: a reader that allocates for it up \
                      front asks for 48 GiB.",
        reason: "malformed",
        chunk: Some(MESH_TAG),
        bytes: huge_vertex_count,
    },
];

/// A file taken apart field by field, to be damaged on purpose.
struct RawFile {
    magic: [u8; 8],
    version: u16,
    chunks: Vec<RawChunk>,
}

/// A chunk as stored: its header fields and stored bytes. The stored length
/// is that of `stored`.
struct RawChunk {
    tag: ChunkTag,
    version: u16,
    raw_len: u64,
    crc: u32,
    stored: Vec<u8>,
}

impl RawFile {
    /// The valid triangle file, taken apart: a scene chunk, then a mesh
    /// chunk.
    fn triangle() -> Self {
        let bytes = write_file(&triangle(), Vec::new()).expect("the triangle writes");
        Self::parse(&bytes)
    }

    fn parse(bytes: &[u8]) -> Self {
        let (header, mut rest) = bytes.split_at(10);
        let mut chunks = Vec::new();
        while !rest.is_empty() {
            let (head, tail) = rest.split_at(26);
            let stored_len = u64::from_le_bytes(head[14..22].try_into().unwrap());
            let (stored, tail) = tail.split_at(usize::try_from(stored_len).unwrap());
            chunks.push(RawChunk {
                tag: ChunkTag(head[..4].try_into().unwrap()),
                version: u16::from_le_bytes(head[4..6].try_into().unwrap()),
                raw_len: u64::from_le_bytes(head[6..14].try_into().unwrap()),
                crc: u32::from_le_bytes(head[22..].try_into().unwrap()),
                stored: stored.to_vec(),
            });
            rest = tail;
        }
        Self {
            magic: header[..8].try_into().unwrap(),
            version: u16::from_le_bytes(header[8..].try_into().unwrap()),
            chunks,
        }
    }

    fn chunk(&mut self, tag: ChunkTag) -> &mut RawChunk {
        let chunk = self.chunks.iter_mut().find(|chunk| chunk.tag == tag);
        chunk.expect("the file has the chunk")
    }

    fn bytes(&self) -> Vec<u8> {
        let mut out = self.magic.to_vec();
        out.extend(self.version.to_le_bytes());
        for chunk in &self.chunks {
            out.extend(chunk.tag.0);
            out.extend(chunk.version.to_le_bytes());
            out.extend(chunk.raw_len.to_le_bytes());
            out.extend((chunk.stored.len() as u64).to_le_bytes());
            out.extend(chunk.crc.to_le_bytes());
            out.extend(&chunk.stored);
        }
        out
    }
}

fn wrong_magic() -> Vec<u8> {
    let mut file = RawFile::triangle();
    file.magic[..5].copy_from_slice(b"bread");
    file.bytes()
}

fn text_mode_magic() -> Vec<u8> {
    let bytes = RawFile::triangle().bytes();
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &byte) in bytes.iter().enumerate() {
        if !(byte == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
            out.push(byte);
        }
    }
    out
}

fn newer_container() -> Vec<u8> {
    let mut file = RawFile::triangle();
    file.version = 2;
    file.bytes()
}

fn cut_short() -> Vec<u8> {
    let mut bytes = RawFile::triangle().bytes();
    bytes.pop();
    bytes
}

fn crc_mismatch() -> Vec<u8> {
    let mut file = RawFile::triangle();
    let stored = &mut file.chunk(MESH_TAG).stored;
    *stored.last_mut().expect("the chunk stores bytes") ^= 1;
    file.bytes()
}

fn wrong_raw_length() -> Vec<u8> {
    let mut file = RawFile::triangle();
    file.chunk(SCENE_TAG).raw_len += 1;
    file.bytes()
}

fn huge_raw_length() -> Vec<u8> {
    let mut file = RawFile::triangle();
    file.chunk(MESH_TAG).raw_len = 1 << 63;
    file.bytes()
}

fn newer_chunk() -> Vec<u8> {
    let mut file = RawFile::triangle();
    file.chunk(MESH_TAG).version = 3;
    file.bytes()
}

/// A chunk to write as it is: its tag, version and payload.
type Payload = (ChunkTag, u16, Vec<u8>);

/// The chunks of the valid triangle file: a scene chunk, then a mesh chunk.
fn triangle_payloads() -> Vec<Payload> {
    let bytes = write_file(&triangle(), Vec::new()).expect("the triangle writes");
    let mut reader = BreadReader::new(bytes.as_slice()).expect("the header reads");
    let mut chunks = Vec::new();
    while let Some(chunk) = reader.next_chunk().expect("the chunk reads") {
        chunks.push((chunk.tag, chunk.version, chunk.payload));
    }
    chunks
}

/// A file of `chunks`, each written as it is.
fn file_of(chunks: &[Payload]) -> Vec<u8> {
    let mut writer = BreadWriter::new(Vec::new()).expect("the header writes");
    for (tag, version, payload) in chunks {
        writer
            .chunk(*tag, *version, payload)
            .expect("the chunk writes");
    }
    writer.finish().expect("the file ends")
}

/// The parts of a version 2 mesh payload without metadata (`SPEC.md`,
/// section 6.6): after the metadata flag, the counts and the coding bytes.
fn mesh_parts(payload: &[u8]) -> Vec<&[u8]> {
    assert_eq!(payload[0], 0, "the mesh has no metadata");
    let mut rest = &payload[11..];
    let mut parts = Vec::new();
    while !rest.is_empty() {
        let (len, tail) = rest.split_at(4);
        let len = usize::try_from(u32::from_le_bytes(len.try_into().unwrap())).unwrap();
        let (part, tail) = tail.split_at(len);
        parts.push(part);
        rest = tail;
    }
    parts
}

/// A version 2 mesh payload without metadata, in connectivity A and the
/// meshopt coding, from its counts and parts.
fn mesh_payload(vertices: u32, triangles: u32, parts: &[&[u8]]) -> Vec<u8> {
    let mut payload = vec![0];
    payload.extend(vertices.to_le_bytes());
    payload.extend(triangles.to_le_bytes());
    payload.extend([0, 0]);
    for part in parts {
        payload.extend(u32::try_from(part.len()).unwrap().to_le_bytes());
        payload.extend(*part);
    }
    payload
}

const IN_ORDER_MESHOPT: MeshCoding = MeshCoding {
    connectivity: Connectivity::InOrder,
    vertices: VertexCoding::Meshopt,
};

/// The vertices of the triangle with the triangles of a mesh of 4 vertices,
/// whose triangle names the fourth one.
fn no_such_vertex() -> Vec<u8> {
    let mut chunks = triangle_payloads();
    let four = Mesh {
        meta: None,
        vertices: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        triangles: vec![[0, 1, 3]],
    };
    let four = encode_mesh_v2_with(&four, IN_ORDER_MESHOPT).expect("the mesh encodes");
    let three = encode_mesh_v2_with(&triangle().scene.meshes[0], IN_ORDER_MESHOPT)
        .expect("the triangle encodes");
    let (four, three) = (mesh_parts(&four), mesh_parts(&three));
    chunks[1].2 = mesh_payload(3, 1, &[four[0], four[1], three[2]]);
    file_of(&chunks)
}

/// The triangle's scene, written by hand with its leaf naming mesh 1.
fn no_such_mesh() -> Vec<u8> {
    let mut chunks = triangle_payloads();
    let mut scene = vec![0]; // no metadata
    scene.extend(1u32.to_le_bytes()); // one mesh chunk
    scene.extend(0u32.to_le_bytes()); // no mesh resources
    scene.extend(1u32.to_le_bytes()); // one record:
    scene.extend([1, 0, 0]); // a leaf, no metadata, no transform,
    scene.extend(1u32.to_le_bytes()); // showing mesh 1
    scene.extend(1u32.to_le_bytes()); // one root:
    scene.extend(0u32.to_le_bytes()); // node 0
    chunks[0].2 = scene;
    file_of(&chunks)
}

fn two_scenes() -> Vec<u8> {
    let mut chunks = triangle_payloads();
    chunks.insert(1, chunks[0].clone());
    file_of(&chunks)
}

fn huge_vertex_count() -> Vec<u8> {
    let mut chunks = triangle_payloads();
    chunks[1].2[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
    file_of(&chunks)
}

/// The reason code of `err`, and the chunk it is about, as the README names
/// them.
fn reason(err: &ReadFileError) -> (&'static str, Option<ChunkTag>) {
    match err {
        ReadFileError::File(ReadBreadError::NotBread) => ("not-bread", None),
        ReadFileError::File(ReadBreadError::UnsupportedVersion(_)) => ("version", None),
        ReadFileError::File(ReadBreadError::Truncated) => ("cut-short", None),
        ReadFileError::File(ReadBreadError::Corrupt(tag)) => ("crc", Some(*tag)),
        ReadFileError::File(ReadBreadError::BadPayload(tag)) => ("bad-payload", Some(*tag)),
        ReadFileError::ChunkVersion { tag, .. } => ("version", Some(*tag)),
        ReadFileError::ChunkCount { tag, .. } => ("chunk-count", Some(*tag)),
        ReadFileError::Mesh(DecodeMeshError::Malformed) => ("malformed", Some(MESH_TAG)),
        ReadFileError::Mesh(DecodeMeshError::VertexOutOfRange) => {
            ("no-such-vertex", Some(MESH_TAG))
        }
        ReadFileError::Scene(DecodeSceneError::Malformed) => ("malformed", Some(SCENE_TAG)),
        other => panic!("no conformance file expects this error: {other}"),
    }
}

/// The JSON that gives the reason of an invalid case.
fn describe_invalid(case: &Invalid) -> String {
    let json = Json::Object(vec![
        ("description", Json::text(case.description)),
        ("reason", Json::text(case.reason)),
        (
            "chunk",
            Json::or_null(case.chunk, |tag| Json::Text(tag.to_string())),
        ),
    ]);
    let mut out = String::new();
    json.write(&mut out, 0);
    out.push('\n');
    out
}

fn invalid_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/invalid")
}

#[test]
fn the_invalid_files_match_their_cases() {
    let dir = invalid_dir();
    for case in INVALID {
        let stored = std::fs::read(dir.join(format!("{}.bread", case.name)))
            .unwrap_or_else(|err| panic!("{}.bread: {err}", case.name));
        assert!(
            stored == (case.bytes)(),
            "{}.bread is not what the case writes",
            case.name
        );

        let Err(err) = read_file(stored.as_slice()) else {
            panic!("{}.bread is read", case.name);
        };
        assert_eq!(
            reason(&err),
            (case.reason, case.chunk),
            "{}.bread is refused as: {err}",
            case.name
        );

        let json = std::fs::read_to_string(dir.join(format!("{}.json", case.name)))
            .unwrap_or_else(|err| panic!("{}.json: {err}", case.name));
        assert_eq!(json, describe_invalid(case), "{}.json", case.name);
    }
}

/// The names of the files in `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the folder reads")
        .map(|entry| entry.expect("the entry reads").file_name())
        .map(|name| name.into_string().expect("the name is UTF-8"))
        .collect();
    names.sort();
    names
}

/// `name.bread` and `name.json` for each name, sorted.
fn case_files<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut files: Vec<String> = names
        .flat_map(|name| [format!("{name}.bread"), format!("{name}.json")])
        .collect();
    files.sort();
    files
}

/// A description is one sentence, with single spaces: a line of it broken
/// without a trailing `\` would carry the next line's indentation.
#[test]
fn descriptions_have_single_spaces() {
    let valid = CASES.iter().map(|case| (case.name, case.description));
    let invalid = INVALID.iter().map(|case| (case.name, case.description));
    for (name, description) in valid.chain(invalid) {
        assert!(
            !description.contains("  ") && description.trim() == description,
            "{name}: {description:?}"
        );
    }
}

#[test]
fn every_file_has_a_case() {
    assert_eq!(
        file_names(&valid_dir()),
        case_files(CASES.iter().map(|case| case.name))
    );
    assert_eq!(
        file_names(&invalid_dir()),
        case_files(INVALID.iter().map(|case| case.name))
    );
}

#[test]
#[ignore = "rewrites the conformance files"]
fn regenerate() {
    let dir = valid_dir();
    std::fs::create_dir_all(&dir).expect("the folder is made");
    for case in CASES {
        let (bytes, json) = generate(case);
        std::fs::write(dir.join(format!("{}.bread", case.name)), bytes).expect("the file writes");
        std::fs::write(dir.join(format!("{}.json", case.name)), json).expect("the JSON writes");
    }

    let dir = invalid_dir();
    std::fs::create_dir_all(&dir).expect("the folder is made");
    for case in INVALID {
        let bytes = (case.bytes)();
        let json = describe_invalid(case);
        std::fs::write(dir.join(format!("{}.bread", case.name)), bytes).expect("the file writes");
        std::fs::write(dir.join(format!("{}.json", case.name)), json).expect("the JSON writes");
    }
}
