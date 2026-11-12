---
title: A third implementation, in C++
status: stable
updated: 2026-11-12
---

Caliper, a mesh repair program by the same author as `.bread`, opens and saves `.bread` files. Its
support was written in C++ from the specification of version 1 alone, without the reference
implementation's code. It is the third implementation of the format, after the reference one in
Rust and the second reader in Python ([second-reader.md](second-reader.md)), and the first that
both reads and writes and is not the reference.

Caliper opens and saves `.bread` since its release 0.9.6, of 2026-11-11, where a `.bread` file also
opens on double-click.

**What it does:**

- it reads the whole scene and every mesh, `MESH` version 1 and version 2;
- it writes `MESH` version 2, choosing the smaller connectivity for each mesh, as the reference
  implementation does;
- it stands on the same libraries the format names: Zstandard and meshoptimizer.

**How it was checked:**

- every conformance file, on Windows and on Linux: the valid ones give exactly what their JSON says,
  and the invalid ones are refused for their reason;
- what Caliper writes, read again by the second reader: every triangle corner comes back as it was
  in the STL, bit for bit;
- a model of 2.4 M triangles goes from 121.8 MB as binary STL to 11.3 MB as `.bread`, 10.7 times
  smaller.

**What it found.** Nothing in the specification was ambiguous. The second reader had found six
places where the text was not enough; once those were spelled out, a third implementation in
another language found none.

One detail it met is not a gap in the specification but worth knowing for anyone writing a writer:
the vertex cache optimisation of meshoptimizer may hand back a triangle rotated, starting at another
of its corners. The format already keeps which corner each triangle starts at, so the triangle
comes back as it went in, whichever way a writer reorders it: `MESH` version 2 lists the triangles
that come out of its codec rotated, and which corner each one starts at.
