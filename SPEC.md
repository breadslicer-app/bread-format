# The `.bread` file format

**Status:** draft. Sections marked "not written yet" are filled in by the issue they name.

## 1. Introduction

A `.bread` file holds triangle meshes, and a scene that places them, as small as they can be
stored without losing anything. It is meant to replace STL wherever a model goes from one program
or person to another, and it has room for data that only some applications use, such as a slicer's
editable supports.

Lossless means that a mesh read back from a `.bread` file is the mesh that was written:

- every vertex coordinate, bit for bit;
- every triangle, in the same order, starting at the same corner.

A `.bread` file holds geometry only: no colours, textures, UV coordinates, materials or animation.

### 1.1 Overview

A file is a short header followed by chunks. Each chunk has a four-byte tag that says what it
holds, a version of its own, and a payload that is compressed and guarded by a checksum. The
[container](#4-container) section describes this layer; the sections after it describe what the
payloads hold.

### 1.2 Terms

- **Writer:** a program that creates `.bread` files.
- **Reader:** a program that opens them.
- **Payload:** the bytes of a chunk once decompressed. The chunk layouts in this document describe
  payloads.

## 2. Conformance

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT",
"RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as
described in BCP 14 ([RFC 2119](https://www.rfc-editor.org/rfc/rfc2119),
[RFC 8174](https://www.rfc-editor.org/rfc/rfc8174)) when, and only when, they appear in all
capitals, as shown here.

Requirements on writers say what a valid file is. Requirements on readers say how to treat any
file, valid or not.

## 3. Conventions

### 3.1 Byte order and types

All multi-byte values are little-endian. Fields follow one another with no padding or alignment.

| Type | Size | Meaning |
|---|---|---|
| `u8`, `u16`, `u32`, `u64` | 1, 2, 4, 8 bytes | unsigned integer |
| `f32` | 4 bytes | IEEE 754 binary32 floating-point number |
| `f64` | 8 bytes | IEEE 754 binary64 floating-point number |

Floating-point values are kept bit for bit. Readers and writers MUST NOT change their bits, for
example by flushing subnormals to zero, turning negative zero into zero, or rewriting NaNs.

### 3.2 Counts and strings

- **Count:** a `u32` giving how many items follow. A list is stored as its count followed by its
  items, unless its section says otherwise, so a list holds at most 2³² − 1 items.
- **Byte string:** a `u32` length, then that many bytes.
- **Optional string:** a `u8` presence flag. `0` means the string is absent, and nothing follows.
  `1` means it is present, and a byte string follows that holds UTF-8 text. An empty string
  (present, length 0) and an absent one are different values.

### 3.3 Malformed payloads

A payload is malformed when:

- it ends before its last field;
- bytes remain after its last field;
- a field holds a value this document does not allow, such as a presence flag other than `0` or
  `1`, or a string that is not valid UTF-8.

A reader MUST report a malformed payload as an error and MUST NOT guess at what it meant.

A reader MUST NOT allocate memory for a count or a length before checking it against the bytes left
in the payload: items that take at least *k* bytes each cannot number more than the bytes left
divided by *k*. This way a damaged count cannot make a reader ask for more memory than the file
holds.

### 3.4 Versions

Versions are `u16` numbers that start at 1 and only go up. They live in two places:

- **The container version**, in the file header, covers the header and the chunk layout. A reader
  MUST refuse a file whose container version is newer than the ones it supports.
- **The chunk version**, in each chunk's header, covers that chunk's payload. It goes up when the
  payload layout changes, independently of every other chunk.

A reader MUST skip a chunk whose tag it does not know and go on reading the file. This is how new
kinds of chunks are added without breaking existing readers.

A reader MUST refuse a file that holds a chunk it knows at a version newer than the ones it
supports. Skipping such a chunk could lose data the rest of the file depends on.

## 4. Container

A file is a header followed by chunks, one after another, until the end of the file. There is no
index, no end marker and no padding between chunks.

```text
header   magic, container version
chunk    tag, chunk version, raw length, stored length, CRC-32, stored bytes
chunk    ...
```

### 4.1 Header

| Offset | Field | Type | Value |
|---|---|---|---|
| 0 | magic | 8 bytes | `42 52 45 41 44 00 0D 0A`: `BREAD`, NUL, CR, LF |
| 8 | container version | `u16` | `1` |

The magic is built the way PNG's is:

- `BREAD` names the format to anyone looking at the bytes;
- the NUL byte makes tools that guess whether a file is text treat it as binary;
- CR LF catches a file that went through a text-mode transfer: turning line endings from CR LF
  into LF, or from LF into CR LF, changes the magic. A transfer that leaves the magic alone but
  changes bytes further on is caught by the chunk checksums.

This document describes container version 1, so every file starts with these ten bytes:
`42 52 45 41 44 00 0D 0A 01 00`.

### 4.2 Chunks

| Offset | Field | Type | Meaning |
|---|---|---|---|
| 0 | tag | 4 bytes | what the chunk holds |
| 4 | chunk version | `u16` | the layout of this chunk's payload |
| 6 | raw length | `u64` | the length of the payload, in bytes |
| 14 | stored length | `u64` | the length of the stored bytes |
| 22 | CRC | `u32` | the CRC-32 of the stored bytes |
| 26 | stored bytes | stored length | the payload, compressed |

Offsets count from the start of the chunk, and the next chunk starts right after the stored bytes.

- **Tag:** four bytes, compared byte for byte. The official tags are four ASCII letters, listed in
  the [chunk registry](#7-chunk-registry), which also says how other programs name chunks of their
  own.
- **Chunk version:** see [Versions](#34-versions).
- The container does not limit how many chunks of a tag a file holds, or their order. Each chunk's
  section does.

### 4.3 Stored bytes and checksum

The stored bytes are one Zstandard frame ([RFC 8878](https://www.rfc-editor.org/rfc/rfc8878)) that
decompresses to the payload.

- A writer MUST NOT write a frame that needs a dictionary.
- A writer SHOULD NOT write a frame whose window is larger than 8 MB. RFC 8878 recommends that
  limit so that every decoder can read the frame.
- The compression level is the writer's choice. A reader does not need to know it, and two writers
  may store the same payload as different bytes.

The CRC is the CRC-32 used by zlib, gzip and PNG: polynomial `0x04C11DB7`, input and output
reflected, initial value `0xFFFFFFFF`, final XOR `0xFFFFFFFF`. As a check, the CRC-32 of the nine
ASCII bytes `123456789` is `0xCBF43926`. It covers the stored bytes only, not the chunk header, and
unlike PNG's it is stored little-endian, like every other field.

### 4.4 Reading a file

A reader checks the header, then reads chunks until the file ends right after a chunk's stored
bytes. For each chunk it reads the 26-byte chunk header and the stored bytes, checks the CRC, and
then skips the chunk, refuses the file, or decompresses the payload, as the table says.

| What the reader finds | What it MUST do |
|---|---|
| fewer than 8 bytes, or 8 bytes that are not the magic | refuse the input: it is not a `.bread` file |
| a container version newer than the ones it supports | refuse the file |
| the file ends inside the container version, a chunk header or the stored bytes | refuse the file: it is cut short |
| a CRC that does not match the stored bytes, in any chunk | refuse the file: it is damaged |
| a tag it does not know | skip the chunk and go on |
| a tag it knows, at a chunk version newer than the ones it supports | refuse the file |
| stored bytes of a chunk it reads that are not a valid frame, or that do not decompress to exactly the raw length | refuse the file: it is damaged |

A reader need not decompress a chunk it skips. If it does, and the stored bytes fail as in the last
row, it MAY refuse the file, since no writer writes such a chunk.

When a reader refuses a file it SHOULD say why, naming the chunk's tag when one chunk is at fault.

### 4.5 Size limits

Lengths come from the file, so a damaged one can be enormous: one flipped bit in a raw length can
ask for 2⁶³ bytes. A reader MUST NOT allocate memory for a recorded length before it holds the bytes
that back it:

- **Stored bytes:** read them as they arrive, stopping at the end of the file, or first check that
  the stored length fits in what is left of the file.
- **Payload:** decompress as a stream and stop as soon as the output goes past the raw length,
  instead of allocating the raw length up front. A content size recorded in the Zstandard frame is
  just as untrusted.

Memory then grows with what the file really holds once decompressed, never with what a damaged
length claims. The same rule applies to the counts inside payloads ([Malformed
payloads](#33-malformed-payloads)).

A reader MAY also set maximums of its own, such as the largest payload or the largest total it
decompresses, or the largest Zstandard window, and refuse a file that goes past them. Such a refusal
SHOULD say that the file is too large for the reader, not that it is damaged. This document sets no
maximum beyond the `u64` lengths.

## 5. Scene (`SCNE`)

The scene chunk places the meshes: a tree of groups and leaves, each node with a transform, where a
leaf shows one mesh. A mesh several leaves show is stored once, and a whole subtree can be shared
too: each reference node places one more copy of it. Every reader can draw all of it. The result of
an operation on several meshes, such as a boolean difference, is stored as a leaf that shows the
resulting mesh.

A file holds exactly one scene chunk. This section describes chunk version 1.

### 5.1 Layout

| Field | Type | Meaning |
|---|---|---|
| has metadata | `u8` | `0`: no name or comment follow. `1`: they do. |
| name | optional string | the scene's name; present only when *has metadata* is `1` |
| comment | optional string | free text about the scene; present only when *has metadata* is `1` |
| mesh count | `u32` | how many `MESH` chunks the file holds |
| mesh resources | count, then a `u32` mesh index each | the meshes the scene keeps as shared ([5.2](#52-records)) |
| records | count, then the records | the nodes and the subtrees, children first ([5.2](#52-records)) |
| roots | count, then a `u32` node index each | the scene's top-level nodes, in order |

The scene names everything by index, each kind of thing numbered from 0 in its own order:

- **meshes:** the `MESH` chunks, in file order. The mesh count MUST equal the number of `MESH`
  chunks in the file, and a reader MUST refuse the file when it does not;
- **mesh resources:** the entries of the mesh resources list, in order;
- **nodes:** the node records, in order. Subtree records do not count;
- **subtrees:** the subtree records, in order.

### 5.2 Records

Each record starts with a `u8` kind:

| Kind | Record | Fields after the kind |
|---|---|---|
| `0` | group | node fields; then a count, and a `u32` node index for each child, in order |
| `1` | leaf | node fields; then a `u32` mesh index |
| `2` | leaf with a shared mesh | node fields; then a `u32` mesh resource index |
| `4` | reference | node fields; then a `u32` subtree index |
| `5` | subtree | a `u32` node index: that node becomes the root of the next subtree |

Any other kind, `3` included, makes the payload malformed.

Records of kinds 0, 1, 2 and 4 are nodes, and start with the same node fields:

| Field | Type | Meaning |
|---|---|---|
| has metadata | `u8` | `0`: no name or comment follow. `1`: they do. |
| name | optional string | the node's name; present only when *has metadata* is `1` |
| comment | optional string | free text about the node; present only when *has metadata* is `1` |
| has transform | `u8` | `0`: the identity, and nothing follows. `1`: twelve `f64` follow. |
| linear part | 9 `f64` | the 3 × 3 matrix *M*, row by row; present only when *has transform* is `1` |
| translation | 3 `f64` | the vector *t*; present only when *has transform* is `1` |

A node's transform takes a point *p* in the node's own space to its parent's space:
*p*′ = *M* *p* + *t*. That is, x′ = m₀₀ x + m₀₁ y + m₀₂ z + tₓ, and likewise for y′ and z′ with the
second and third rows.

Both kinds of leaf show one mesh. A kind 2 leaf names it through the mesh resources list: a mesh the
scene keeps as shared on purpose, so that a program editing it changes every leaf that shows it. A
mesh that several kind 1 leaves show is also stored only once.

### 5.3 What a reader checks

Records are listed children first: every record names only nodes, mesh resources and subtrees
listed before it. So no node can contain itself, and no subtree can reach itself through references,
without any further check.

Each node has at most one place in the scene: one group's child, a root, or a subtree's root. A
reader MUST refuse the payload as malformed when:

- a mesh resource, a record or a root names a mesh, mesh resource, node or subtree that does not
  exist at that point of the payload;
- a node is given a second place: listed as a child twice, in the same group or in two, or as a
  child and a root, a root twice, or a subtree's root and anything else.

A node that has no place is not part of the scene: a writer SHOULD NOT list one, and a reader
ignores it.

### 5.4 Placing the meshes

To place the meshes, walk down from each root, in order, starting with the identity transform. Each
node's world transform is its parent's world transform composed with its own:
*W*(node) = *W*(parent) ∘ *T*(node), where (*A* ∘ *B*)(*p*) = *A*(*B*(*p*)).

- A **group** passes its world transform on to each of its children.
- A **leaf** shows its mesh with its world transform: each vertex *v* goes to *W*(leaf)(*v*).
- A **reference** shows one more copy of its subtree: the walk goes on into the subtree's root as
  if it were the reference's only child. A subtree that no reference names is not shown.

A mesh that no leaf shows, such as a mesh resource that no leaf names, is kept in the file but not
shown.

Transforms are `f64` and vertices `f32`. A reader SHOULD compute world positions in `f64`, and
round to `f32`, if it needs to, only at the end.

## 6. Mesh (`MESH`)

A `MESH` chunk holds one triangle mesh: its vertex positions, and triangles that name them. The
scene names meshes by the order of their `MESH` chunks. This section describes chunk version 1.

### 6.1 What is kept

A mesh read back is the mesh that was written:

- the same vertices, in the same order, every coordinate bit for bit;
- the same triangles, in the same order, each starting at the same corner, so winding and first
  corner are both kept;
- its name and comment, when it has them.

A writer MUST store vertices and triangles in the mesh's own order, and MUST NOT reorder them to
compress better.

Facet normals that a source file declares, such as the one STL stores with every triangle, are not
kept: they are file data, not geometry. A program that needs them computes them from the triangle's
corners.

### 6.2 Layout

| Field | Type | Meaning |
|---|---|---|
| has metadata | `u8` | `0`: no name or comment follow. `1`: they do. |
| name | optional string | the mesh's name; present only when *has metadata* is `1` |
| comment | optional string | free text that came with the mesh, such as an STL header; present only when *has metadata* is `1` |
| vertex count | `u32` | how many vertices the mesh has |
| vertex buffer | byte string | the vertices, encoded ([6.3](#63-vertices)) |
| triangle count | `u32` | how many triangles the mesh has |
| index buffer | byte string | the triangles, encoded ([6.4](#64-triangles)) |
| rotations | count, then 5 bytes each | the triangles to turn back ([6.5](#65-rotations)) |

A *has metadata* flag other than `0` or `1` makes the payload malformed. A mesh with metadata whose
name and comment are both absent is a different value from a mesh with no metadata.

### 6.3 Vertices

A vertex is three `f32`, x then y then z: 12 bytes, little-endian like every value in this
document. The vertex buffer holds all of them, in order,
encoded with **version 1 of the attribute codec**: the bitstream "Mode 0: attributes" in Appendix A
of the glTF extension `KHR_meshopt_compression`, as published at
[commit `3e9dfff`](https://github.com/KhronosGroup/glTF/blob/3e9dfff91d52cd79fa0169e8453b7fdaca663fd8/extensions/2.0/Khronos/KHR_meshopt_compression/README.md#mode-0-attributes)
of the Khronos glTF repository, with:

- a byte stride of 12, and as many elements as the vertex count;
- no filter: the decoded bytes are the vertices themselves.

The codec works on bytes and keeps every one of them, so it keeps every bit of every coordinate.

- The buffer's first byte MUST be `0xA1`. A reader MUST refuse any other first byte as malformed,
  including `0xA0`: version 0 of the codec is published too, but this chunk version does not use
  it. A writer MUST write version 1, whatever its encoder's default.
- Every block of up to 256 vertices starts with a 3-byte control header, so a buffer of *L* bytes
  holds fewer than 256 × *L* / 3 vertices. A larger vertex count is malformed, and a reader MUST
  refuse it before allocating for it ([Malformed payloads](#33-malformed-payloads)).
- A buffer that ends before every vertex is decoded, or that has bytes left over before its tail
  block, is malformed.

### 6.4 Triangles

A triangle is three `u32` indices into the vertices, starting at 0. The index buffer holds the
indices of all triangles, in order, encoded with **version 1 of the triangle codec**: the bitstream
"Mode 1: triangles" in Appendix A of the glTF extension `EXT_meshopt_compression`, as published at
[commit `5ccac1c`](https://github.com/KhronosGroup/glTF/blob/5ccac1cb0f85197c4f78c3ddff0b12cdccd79da3/extensions/2.0/Vendor/EXT_meshopt_compression/README.md#mode-1-triangles)
of the Khronos glTF repository (`KHR_meshopt_compression` defines the same bitstream), with:

- a byte stride of 4, and three times the triangle count as the element count.

Then:

- The buffer's first byte MUST be `0xE1`. A reader MUST refuse any other first byte as malformed.
- Every triangle takes at least one byte, besides the first byte and a 16-byte tail block, so a
  buffer of *L* bytes holds at most *L* − 17 triangles. A larger triangle count is malformed, and a
  reader MUST refuse it before allocating for it.
- A buffer that does not decode to exactly the triangle count is malformed.
- Every index MUST be less than the vertex count. A reader MUST refuse a triangle that names a
  vertex the mesh does not have.

### 6.5 Rotations

The triangle codec may store a triangle starting at another of its corners: a triangle (a, b, c)
can come back from the index buffer as (b, c, a) or (c, a, b), with the same winding. The rotation
list names those triangles and turns each one back.

| Field | Type | Meaning |
|---|---|---|
| gap | `u32` | for the first rotation, the triangle's index; for each later one, how far past the previous rotation's triangle it is |
| corner | `u8` | `1` or `2`: which corner of the decoded triangle is the first one |

A decoded triangle (s₀, s₁, s₂) with corner *c* becomes (s*c*, s*c*+1, s*c*+2), indices taken
modulo 3. So corner 1 turns (c, a, b) back into (a, b, c), and corner 2 does the same for
(b, c, a).

- A writer lists every triangle that came back rotated, and only those, in increasing order. So
  every gap after the first is at least 1.
- A reader MUST refuse as malformed a rotation that names a triangle past the last one, a corner
  other than `1` or `2`, or a gap of 0 after the first rotation.

## 7. Chunk registry

Every tag a writer writes MUST be four ASCII letters or digits, the first of them a letter. The case
of that first letter says who the tag belongs to:

- **Uppercase (`A`–`Z`): official.** The tag is listed below, and this document describes it. A
  writer MUST NOT write an uppercase tag that is not listed.
- **Lowercase (`a`–`z`): private.** A program names chunks of its own this way, and no official
  chunk will ever use such a tag ([7.2](#72-private-chunks)).

A reader skips every tag it does not know, whatever its bytes ([Reading a
file](#44-reading-a-file)).

### 7.1 Official chunks

| Tag | Version | Per file | Holds | Section |
|---|---|---|---|---|
| `SCNE` | 1 | exactly one | the scene: its nodes, their transforms, and the meshes they use | [5](#5-scene-scne) |
| `MESH` | 1 | any number | one triangle mesh | [6](#6-mesh-mesh) |
| `PROJ` | 1 | at most one | application data: project settings | [7.3](#73-application-chunks) |
| `SUPP` | 2 | at most one | application data: editable supports | [7.3](#73-application-chunks) |
| `HOLW` | 2 | at most one | application data: hollowed models | [7.3](#73-application-chunks) |

*Version* is the chunk version this document describes. A reader that reads a tag MUST refuse a file
that holds a different number of chunks of that tag than *Per file* allows.

A new official chunk, or a new version of one, is added to this table by a change to this document.

### 7.2 Private chunks

A program can store data of its own in a private chunk, without an official tag:

- A private chunk MUST NOT change what the official chunks mean. A reader that skips it MUST still
  read the rest of the file right: the same meshes, placed by the same scene.
- Two programs may choose the same private tag. A program that reads its own private chunks SHOULD
  make sure that a payload is its own, for example by starting it with the program's name, and skip
  the chunk when it is not.
- The chunk version of a private chunk belongs to the program that writes it.

A program that edits a file and writes it back MAY drop the chunks it does not know, official or
private. Nothing guarantees that a chunk survives another program's edits.

### 7.3 Application chunks

Application chunks carry documents that belong to one program, such as a slicer's editable
supports. Their tags are official, so they never clash, but this document describes only their
envelope: which parts of the scene a document is about, the role each part plays, and who wrote the
document. What a document holds is up to the program that wrote it.

A reader that does not use them MAY skip them, and the file still reads right: the scene and the
meshes say everything such a reader shows. Below, each chunk says what that is. The three
application chunks so far come from Breadslicer, the slicer the format started in.

#### `PROJ`: project settings

The payload is Breadslicer's project settings document, with no envelope: a UTF-8 TOML document
that starts with its own version, holding the printer, the resin and the export format. Another
program SHOULD keep settings of its own in a private chunk ([7.2](#72-private-chunks)), not in
`PROJ`.

A reader that skips `PROJ` sees the same scene, without the settings it was prepared with.

#### `SUPP` and `HOLW`: the envelope

This section describes version 2 of both chunks. Version 1, used only while this document was a
draft, named a single leaf per entry. A reader that meets it MAY skip it, as it may skip any
application chunk.

Both chunks are a list of entries. Each entry is about one model, the leaf of the model as it
prints, and names one more part of the scene, whose role each chunk below gives:

| Field | Type | Meaning |
|---|---|---|
| entries | count | how many entries follow |
| model | `u32` | the node index of the model's leaf in the scene chunk ([5](#5-scene-scne)) |
| part | `u32` | in `SUPP`, the node index of the support leaf; in `HOLW`, the mesh index of the original ([5.1](#51-layout)) |
| generator | byte string | the name of what wrote the document, as UTF-8 text, such as `df` |
| document version | `u32` | the generator's own version of the document |
| document | byte string | the document |

The last five fields repeat for every entry, so each entry takes at least 20 bytes. A model can have
an entry in each chunk: a hollowed model with supports has one in `HOLW` and one in `SUPP`. A reader
that reads these chunks:

- MUST refuse a chunk as malformed if a generator name is not valid UTF-8;
- MUST refuse the file if a model or a support leaf names no node of the scene, or a node that is
  not a leaf, or if an original names no mesh;
- MUST refuse the file if two entries of one chunk name the same model;
- MUST refuse the file if two entries name the same support leaf, or if a support leaf is also the
  model of an entry in either chunk;
- SHOULD treat an entry whose generator it does not know, or whose document version is newer than
  the ones it supports, as if the entry were absent.

#### `SUPP`: supports

Each entry names the model the supports stand on and the leaf that shows them. Its document
describes the supports: tips, trunks, joints, rafts and the settings they were built with. The
support leaf is stored with an empty mesh, one with no vertices and no triangles, and the program
that wrote the document rebuilds the mesh from it, so the supports stay editable.

A reader that skips `SUPP` sees the support leaves with no mesh: it shows the models without their
supports. This document does not yet give such readers a preview of the supports.

#### `HOLW`: hollowed models

Each entry names a hollowed model and its original, the mesh the model was hollowed from. The
model's leaf shows the hollow, which is what prints. The document describes the rest of how it was
made, the hollowing settings and the drain holes, so the program that wrote it can hollow the
original again with new settings or drains.

The original is a mesh of the file like any other, but usually no leaf shows it: a reader that does
not use `HOLW` keeps it without drawing it ([5.4](#54-placing-the-meshes)), so it never appears on
top of the hollow. A leaf may still show it, for example a solid copy of the same model, since a
mesh several leaves show is stored once.

A reader that skips `HOLW` sees the hollowed models, as they will print, without their originals.
