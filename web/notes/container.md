---
title: The container
status: stable
updated: 2026-11-11
---

A file is a 10-byte header, then chunks one after another until the file ends. There is no index,
no end marker and no padding. All numbers are little-endian, with no alignment.

**The magic** is 8 bytes, `42 52 45 41 44 00 0D 0A`: `BREAD`, NUL, CR, LF. Two bytes follow, the
container version as a `u16`, which is 1 today. The magic is built the way PNG's is:

- `BREAD` names the format to anyone looking at the bytes;
- the NUL byte makes tools that guess whether a file is text treat it as binary;
- CR LF catches a file that went through a text-mode transfer, since turning line endings into LF,
  or into CR LF, changes the magic. Anything a transfer breaks further on is caught by the chunk
  checksums.

**A chunk** has a short header and then its stored bytes. The header gives:

- a four-byte tag that says what the chunk holds;
- a version of its own;
- the raw length and the stored length;
- a CRC-32 of the stored bytes.

The stored bytes are one Zstandard frame that decompresses to the payload. The frame needs no
dictionary, and its window is at most 8 MB, so every decoder can read it. The compression level is
the writer's choice, so two writers may store the same payload as different bytes.

**The checksum** is the CRC-32 of zlib, gzip and PNG, over the stored bytes only, and stored
little-endian:

$$g(x) = \texttt{0x04C11DB7}\ (\text{reflected: } \texttt{0xEDB88320}),\quad \text{init} = \text{xorout} = \texttt{0xFFFFFFFF}$$

$$\operatorname{CRC-32}(\text{"123456789"}) = \texttt{0xCBF43926}$$

**What a reader must do:**

| It finds | It must |
|---|---|
| fewer than 8 bytes, or 8 bytes that are not the magic | refuse: it is not a `.bread` file |
| a container version newer than it knows, or 0 | refuse the file |
| the file ends inside a header or inside stored bytes | refuse: it is cut short |
| a CRC that does not match, in any chunk | refuse: it is damaged |
| a tag it does not know | skip the chunk and go on |
| a known tag at a newer chunk version, or at version 0 | refuse the file |
| stored bytes that are not one valid frame, or that do not decompress to exactly the raw length | refuse: it is damaged |

Skipping unknown tags is how new kinds of chunks arrive without breaking old readers. Refusing a
known tag at a newer version is the other half: skipping it could lose data the rest of the file
depends on.

**Tags.** A tag is four ASCII letters or digits, the first a letter, and the case of that letter
says who owns it. Uppercase tags are official and listed in the registry. Lowercase tags are
private, for a program's own data, and no official chunk will ever use one. A private chunk may not
change what the official chunks mean, so a reader that skips it still reads the same meshes in the
same scene.

**The registry:**

| Tag | Version | Per file | Holds |
|---|---|---|---|
| `SCNE` | 1 | exactly one | the scene: nodes, transforms and the meshes they use |
| `MESH` | 2 | any number | one triangle mesh; version 1 is still read |
| `THMB` | 1 | at most one | a thumbnail, as a PNG of up to 512 × 512 |
| `PROJ` | 1 | at most one | application data: project settings |
| `SUPP` | 2 | at most one | application data: editable supports |
| `HOLW` | 2 | at most one | application data: hollowed models |
| `SPRV` | 1 | at most one | a preview of the supports, for readers that skip `SUPP` |
| `SURF` | 1 | at most one | paint on the faces of models |
