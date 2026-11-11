---
title: Conformance
status: stable
updated: 2026-11-11
---

A format is what a second implementation can read. Every rule of the specification comes with files
that prove a reader follows it.

- **Valid files**, each with a JSON of what it holds: the scene, and each mesh's counts and the
  SHA-256 of its vertices and triangles. A reader passes when what it reads matches the JSON.
- **Invalid files**, each a valid file with one thing changed, and the reason a reader must refuse
  it: `not-bread`, `version`, `cut-short`, `crc`, `bad-payload`, `chunk-count`, `malformed` or
  `no-such-vertex`.

The counts are in `facts.toml` (`conformance_valid`, `conformance_invalid`).

**The reference implementation** is a Rust crate, `bread-format`, with the same code compiled to
WebAssembly for JavaScript. Every change runs its tests on Linux and on Windows before it is merged,
and the three platforms write the same bytes for the same file.

**A second reader.** A reader in plain Python, written from the specification alone, reads every
conformance file the way the reference reader does ([second-reader.md](second-reader.md)).

**Real files.** Beyond the conformance files, real Breadslicer projects are opened, written back
and read again, and every mesh comes back bit for bit (`facts.toml`: `real_projects`).
