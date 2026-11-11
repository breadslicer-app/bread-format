# Changelog

What changes in each release of the `bread-format` crate. The format itself is described in
[SPEC.md](SPEC.md), version 1 since the tag `spec-v1`. Files valid by version 1 open in every
later release. Before the freeze, a release could read files of an older release differently,
and each release said how.

## Unreleased

- **Stored bytes must be exactly one Zstandard frame** (`SPEC.md`, section 4.3). The reader used to
  accept several frames in a row, or a frame followed by a skippable one, as long as they
  decompressed to the raw length; such a chunk is now refused as a bad payload, as the spec always
  said. No file written by this crate has one. New conformance file: `invalid/two-frames`.

## 0.3.0 (2026-11-11)

For use as a git dependency by tag, like 0.2.0. Files open and are written exactly as in 0.2.0,
except that a file of container version 0 is now refused.

- **Container version 0 is refused** (`SPEC.md`, section 3.4), as a chunk at version 0 already was:
  versions start at 1. No file written by this crate has it.
- **Faster writing and reading, same bytes.** Meshes are encoded and decoded side by side on the
  machine's threads, and so are the independent steps inside one `MESH` v2 mesh: finding
  connectivity B's order, trying the three vertex codings, and decoding the triangles beside the
  vertex parts. `write_file` writes exactly the bytes it wrote before. A file refused by
  `read_file` is refused for the same reason as before: the first fault in file order. On 15 real
  Breadslicer projects, writing takes 18–58 % less time and reading up to 50 % less. A single
  scattered mesh of 1.9 M triangles writes in 1.12 s instead of 1.45 s. On WebAssembly everything
  stays on one thread, as before.

## 0.2.0 (2026-11-11)

For use as a git dependency by tag, like 0.1.0. **Reading files of 0.1.0:** they open as before,
their meshes are `MESH` version 1 and are still read. **Writing:** meshes are written as version 2,
which 0.1.0 refuses, so a file written by 0.2.0 does not open in 0.1.0.

- **Thumbnail** (`SPEC.md`, section 8): `BreadFile::thumbnail` carries the `THMB` chunk, a PNG,
  without decoding it. `write_file` writes it as the first chunk. A file with two thumbnail chunks
  is refused. `BreadFile` has a new field, so code that builds one with every field named needs
  `thumbnail: None`.
- **Support preview** (`SPEC.md`, section 9): `BreadFile::support_previews` carries the `SPRV`
  chunk, the mesh of each support leaf's supports for readers that do not rebuild them, as
  `SupportPreview { leaf, mesh }`. `write_file` writes it after the mesh chunks. Each preview must
  name the support leaf of a supports entry, a leaf with an empty mesh, and no leaf twice; this is
  checked when reading and when writing. A file whose supports chunk is a draft (version 1) opens
  without its previews. `BreadFile` has a new field, so code that builds one with every field named
  needs `support_previews: Vec::new()`.
- **Mesh chunk version 2** (`SPEC.md`, sections 6.6 to 6.9): `decode_mesh_v2` and
  `encode_mesh_v2_with` (with `MeshCoding`, `Connectivity` and `VertexCoding`), and `read_file`
  reads `MESH` chunks of versions 1 and 2. `MESH_VERSION` is now 2, the newest version read.
  `EncodeMeshError` has a new variant, `Unsupported`, for a mesh a chosen coding cannot store.
- **`write_file` writes `MESH` version 2**, through `encode_mesh_v2`, which picks the codings that
  store each mesh smallest. Meshes come back bit for bit, in the same order, but a file read and
  written back no longer has the same bytes when its meshes were version 1. On Breadslicer's own
  files, about 12–15% smaller; saving a mesh of 2 million triangles takes about half a second.
- **Surface paint** (`SPEC.md`, section 10): `BreadFile::surface` carries the `SURF` chunk, paint on
  the faces of leaves, as `SurfaceEntry { leaf, kind, version, document }`. Only the envelope is
  checked (each entry names a leaf, no two share a leaf and a kind); documents are carried without
  being read. `write_file` writes it after the hollows and before the mesh chunks. `BreadFile` has a
  new field, so code that builds one with every field named needs `surface: Vec::new()`.
- **`supported_version(tag)`**: the newest version of a chunk this build reads, or `None` for a tag
  it does not know and skips. `read_file` uses it, so other readers built on `BreadReader`, such as
  the JavaScript binding, check chunk versions the same way.

## 0.1.0 (2026-10-30)

The first release, for use as a git dependency by tag. It is not on crates.io yet.

- **Container** (`SPEC.md`, section 4): reads and writes the header and the chunks, each with its
  tag, version, zstd-compressed payload and CRC-32. A damaged file is refused, and a damaged count
  cannot make a reader reserve more memory than the file could fill.
- **Meshes** (section 6): `Mesh`, `encode_mesh` and `decode_mesh`. Vertices come back bit for bit,
  and triangles in their order, each with its first corner.
- **Scene** (section 5): `Scene`, `Node`, `NodeKind` and `Transform`, with groups, leaves, shared
  meshes and references to subtrees. Everything is kept by index, in the order of the file.
- **Application chunks** (section 7.3): the project settings (`PROJ`, carried as it is), and the
  supports (`SUPP`) and hollows (`HOLW`) at version 2. Each supports entry names its model and the
  leaf that shows the supports; each hollows entry names its model and the original mesh. The
  rules of section 7.3 are checked when reading and when writing.
- **Whole files:** `BreadFile`, `read_file` and `write_file`. Reading skips chunks it does not
  know, and the supports and hollows chunks of version 1, which only drafts of the format wrote.
