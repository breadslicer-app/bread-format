/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The conformance files in `conformance/valid/` at the root of the
//! repository: small `.bread` files every reader must open, each with a JSON
//! file beside it that describes what it holds (see the README there).
//!
//! The cases below make the files. `the_files_match_their_cases` checks that
//! writing each case still gives the stored bytes, so a change to them is
//! seen, and that reading the stored file gives its JSON. After a change on
//! purpose, the files are made again with:
//!
//! ```text
//! cargo test -p bread-format --test conformance -- --ignored
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use bread_format::{
    BreadFile, BreadReader, BreadWriter, ChunkTag, HollowEntry, MESH_TAG, Mesh, Meta, Node,
    NodeKind, Scene, SupportEntry, Transform, read_file, write_file,
};
use sha2::{Digest, Sha256};

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

fn rotated_triangle() -> BreadFile {
    one_leaf(None, None, triangle_mesh([1, 2, 0]))
}

fn cube() -> BreadFile {
    one_leaf(named("cube scene"), named("cube"), cube_mesh("cube mesh"))
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

/// The JSON that describes `file`.
fn describe(description: &str, file: &BreadFile) -> String {
    let scene = &file.scene;
    let settings = Json::or_null(file.settings.as_deref(), |settings| {
        Json::Object(vec![
            ("size", Json::index(settings.len())),
            ("sha256", sha256(settings.iter().copied())),
        ])
    });
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
            let payload = &chunk.payload;
            let count = &payload[payload.len() - 9..payload.len() - 5];
            assert_eq!(count, 1u32.to_le_bytes(), "the codec rotates the triangle");
            return;
        }
    }
    panic!("the file has no mesh chunk");
}

#[test]
fn every_file_has_a_case() {
    let mut names: Vec<String> = std::fs::read_dir(valid_dir())
        .expect("the folder reads")
        .map(|entry| entry.expect("the entry reads").file_name())
        .map(|name| name.into_string().expect("the name is UTF-8"))
        .collect();
    names.sort();
    let mut expected: Vec<String> = CASES
        .iter()
        .flat_map(|case| {
            [
                format!("{}.bread", case.name),
                format!("{}.json", case.name),
            ]
        })
        .collect();
    expected.sort();
    assert_eq!(names, expected);
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
}
