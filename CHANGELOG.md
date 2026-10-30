# Changelog

What changes in each release of the `bread-format` crate. The format itself is described in
[SPEC.md](SPEC.md), which is still a draft: until version 1 of the format is frozen, a release may
read files of an older release differently, and each release says how.

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
