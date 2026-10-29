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

*Not written yet (#2).*

## 5. Scene (`SCNE`)

*Not written yet (#5).*

## 6. Mesh (`MESH`)

*Not written yet (#3).*

## 7. Chunk registry

*Not written yet (#7, #8).*
