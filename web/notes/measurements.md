---
title: How the numbers were measured
status: stable
updated: 2026-11-11
---

**The machine.** A desktop PC. Native numbers come from the reference implementation built in
release mode. JavaScript numbers come from the WebAssembly module in Node 26, on V8, the engine of
Chrome. Times are the best of several runs unless they say otherwise.

**Sizes** are decimal: 1 MB = 1,000,000 bytes. The exact bytes are in `data/figure-sizes.json`.

**The figure.** One mesh of 1,898,864 triangles and 949,406 vertices, a figure from a real project,
stored seven ways, all of them lossless:

- **binary STL:** 84 bytes of header and count, then 50 bytes per triangle. Each triangle has a
  facet normal computed from its corners and normalized, as STL writers do;
- **OBJ:** a `v` line per vertex, each coordinate the shortest decimal that reads back as the same
  `f32`, and an `f` line per triangle;
- **zip:** the binary STL, Deflate at level 9;
- **7z:** the binary STL, 7-Zip at `-mx=9`;
- **OBJ + Zstandard:** the OBJ at level 19;
- **`.bread`:** the mesh as a `MESH` chunk of version 1 and of version 2, as the reference
  implementation compresses it (Zstandard at level 3). A whole file adds a 10-byte header and a
  scene chunk of a few dozen bytes.

The `.bread` files are read back and compared bit for bit.

**MESH version 2 against version 1.** The corpus of 22 meshes (255 MB as binary STL) covers
figures, sculptures, scans, CAD parts and the supports of large models. It was measured while
version 2 was designed, and the reference implementation gives the same sizes. No mesh gets larger,
because the meshopt coding stays one of the choices. CAD parts gain the least, since their flat
faces leave little to predict.

**Real projects.** 15 Breadslicer projects, each opened, written back with the reference
implementation and read again. Every mesh comes back bit for bit. Two of them shrink much more than
the rest, because they also held a draft chunk from before the format settled, which is no longer
written back.
