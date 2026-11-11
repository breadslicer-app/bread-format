---
title: Status
status: stable
updated: 2026-11-11
---

The specification is a draft, and close to version 1. Once version 1 is frozen, the container,
`SCNE` and `MESH` change only through new versions, and every reader keeps opening version 1 files.

**Before the freeze:**

- [x] **Units and axes:** millimetres, right-handed, Z up, XY as the build plate.
- [x] **A second reader, written from the specification alone**, that reads every conformance file
  the way the reference reader does ([second-reader.md](second-reader.md)).
- [ ] **Breadslicer on the reference implementation** for its own files. The freeze does not wait for
  it: Breadslicer follows the format, not the other way around.
- [ ] **A full read-through** of the specification, then the tag `spec-v1`.

Until then the full text of the specification is not published (`rules.md`).
