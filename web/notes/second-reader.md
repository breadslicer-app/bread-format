---
title: A second reader, written from the specification alone
status: stable
updated: 2026-11-11
---

A format is what someone else can read from its specification. To show that `.bread` is one, a
second reader was written in plain Python from the specification alone, and from the published
bitstream of the meshoptimizer codecs, without looking at the reference implementation's code. The
reference implementation is Rust, and its JavaScript package is the same code built to WebAssembly,
so neither can show this.

**What it reads:** the whole format. That is the container, the scene and the walk that places its
meshes, `MESH` version 1 and version 2, and the slicer chunks. Version 2 includes both
connectivities, the vertex prediction, rANS and the byte planes. It reads; it does not write.

**How it was checked:**

- every conformance file: the valid ones give exactly what their JSON says, and the invalid ones are
  refused for their reason;
- real files: a `MESH` version 2 mesh of 1.97 M triangles, and projects in version 1, come out with
  the same vertices and triangles as from the reference reader, bit for bit. That includes the
  vertex prediction, computed in `f64` on both sides;
- the meshopt attribute decoder against the official meshoptimizer encoder, at every level, on
  random data. Every control mode, every delta width and every channel mode give the input back
  exactly, including some the conformance files never use.

**What it found.** In six places the text was not enough, and the reader had to work out what was
meant. Each one is now spelled out in the specification:

1. where the tail of the attribute codec's buffer starts;
2. how many bytes a literal block of the attribute codec holds: one per element, not a whole group
   of 16;
3. which way the attribute codec's XOR channel mode rotates;
4. how the rANS writer ends its stream;
5. that version 0 is no version, for the container as for a chunk;
6. which reason a reader gives when one chunk names something another does not have.

The first three are details the published meshoptimizer bitstream leaves to its own implementation.
While checking the fifth, the reference implementation itself turned out to accept a file of
container version 0. It now refuses one, and a conformance file tests it.

**What it is for.** It is plain Python with nothing but the standard library (Python 3.14 or later),
written to be read. That makes it a starting point for importing `.bread` into other programs, many
of which take plug-ins in Python. It favours being clear over being fast: the 1.97 M triangle mesh
takes about half a minute, where the reference implementation takes a fraction of a second.
