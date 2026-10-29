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

*Not written yet (#5).*

## 6. Mesh (`MESH`)

*Not written yet (#3).*

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
| `SUPP` | 1 | at most one | application data: editable supports | [7.3](#73-application-chunks) |
| `HOLW` | 1 | at most one | application data: hollowed models | [7.3](#73-application-chunks) |

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

*Not written yet (#8).*
