# Conformance files

Small `.bread` files for testing a reader against [SPEC.md](../SPEC.md).

## Valid files

Every file in [`valid/`](valid) is a valid `.bread` file, and a conforming reader MUST open it.
Beside each `name.bread` is a `name.json` that says what the file holds. A reader passes a file when
what it reads matches the JSON.

| Field | Meaning |
|---|---|
| `description` | what the file shows, in a sentence |
| `meta` | the scene's metadata: `null`, or an object with `name` and `comment`, each a string or `null` |
| `meshes` | one object per `MESH` chunk, in file order (below) |
| `mesh_resources` | the mesh resources list: a mesh index each |
| `nodes` | one object per node record, in order (below) |
| `subtrees` | the root node index of each subtree record, in order |
| `roots` | the scene's root node indices, in order |
| `settings` | the `PROJ` payload: `null` when there is none, or its `size` in bytes and its `sha256` |
| `supports` | the `SUPP` entries, in order (below) |
| `hollows` | the `HOLW` entries, in order (below) |

Each mesh:

| Field | Meaning |
|---|---|
| `meta` | the mesh's metadata, as for the scene |
| `vertex_count`, `triangle_count` | how many vertices and triangles the mesh has |
| `vertices_sha256` | the SHA-256 of the vertices: x, y and z of each vertex in order, each as the 4 bytes of its `f32`, little-endian |
| `triangles_sha256` | the SHA-256 of the triangles: the three vertex indices of each triangle in order, first corner first, each as a `u32`, little-endian |

Each node has a `kind`, its `meta` as for the scene, and its `transform`: `null` for a node stored
without one (the identity), or an object with `linear`, the 3 × 3 matrix as three rows, and
`translation`. Each number is the shortest decimal that reads back as the same `f64`. Then, by kind:

| `kind` | Field | Meaning |
|---|---|---|
| `"group"` | `children` | the child node indices, in order |
| `"leaf"` | `mesh` | the mesh index |
| `"shared leaf"` | `resource` | the mesh resource index |
| `"reference"` | `subtree` | the subtree index |

Each supports entry has `model` and `support`, the two leaves' node indices; each hollows entry has
`model` and `original`, the original's mesh index. Both then have the `generator` name, its document
`version`, and the document's `document_size` in bytes and its `document_sha256`.

## Making the files again

The files are made by the cases in
[`crates/bread-format/tests/conformance.rs`](../crates/bread-format/tests/conformance.rs). Its tests
check that the crate still writes the same bytes for each case and reads each file as its JSON says.
After a change on purpose, the files are made again with:

```text
cargo test -p bread-format --test conformance -- --ignored
```
