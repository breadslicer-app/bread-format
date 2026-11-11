---
title: Status
status: stable
updated: 2026-11-11
---

Version 1 of the specification is frozen. From here on, the container and every chunk change only
through new versions, never by editing version 1, and a file valid by version 1 stays valid. A
reader that supports later versions keeps opening version 1 files.

**What came before the freeze:**

- [x] **Units and axes:** millimetres, right-handed, Z up, XY as the build plate.
- [x] **A second reader, written from the specification alone**, that reads every conformance file
  the way the reference reader does ([second-reader.md](second-reader.md)).
- [x] **A full read-through** of the specification, then the tag `spec-v1`. It made the text
  clearer in seven places and settled one rule the two readers read differently: a chunk's stored
  bytes are exactly one Zstandard frame.
- [ ] **Breadslicer on the reference implementation** for its own files. The freeze did not wait for
  it: Breadslicer follows the format, not the other way around.
