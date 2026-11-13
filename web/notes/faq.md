---
title: Questions
status: stable
updated: 2026-11-13
---

**Why not just zip the STL?** Because a zip only finds repeated bytes, and an STL's bytes hardly
repeat: every corner is written again for every triangle that touches it, and the facet normals
add noise. `.bread` stores each vertex once and predicts it from its neighbours. The figure of
1.9 M triangles is 39.3 MB as a zipped STL and 6.6 MB as `.bread`; the comparison over public
models, with Zstandard too, is in [formats.md](formats.md). A zip also has to be unpacked before
anything can read the model.

**Why not 3MF?** 3MF is a good format for a whole print job: colours, materials, several objects
on a build plate. Its meshes are XML in a zip, so each coordinate is a decimal number. Whether a
3MF loses bits depends on how many digits its writer prints; nothing in the format keeps them.
`.bread` keeps every coordinate as its 32 bits, by definition, and is smaller and faster to read
([formats.md](formats.md)). The two do different jobs: `.bread` is for handing a model on, exactly
as it is.

**Is it really lossless? How can I check?** Every vertex coordinate comes back bit for bit,
negative zero and NaN payloads included, and every triangle in its order, from its first corner.
Ways to check:

- the viewer ([viewer.md](viewer.md)) reads back every file it converts and compares every corner
  of every triangle with the STL before it offers the file;
- `tools/corpus-bench` does the same on a folder of STLs, and did it on all 2,288 of MZ4250's
  miniatures ([mz4250-corpus.md](mz4250-corpus.md));
- converting back to STL gives every corner exactly as before. The file is not the same byte for
  byte, because `.bread` does not keep the STL's facet normals and attribute bytes (next answer).

**What does it not store, and why?** Colours, textures, UV coordinates, materials and animation:
that is glTF's job, and `.bread` does not compete with it. The facet normals STL stores with every
triangle: they are file data, not geometry, and any program computes them from the corners, which
keep their order. Nor the two attribute bytes STL keeps per triangle: they have no
standard meaning.

**What is the license?** The format, the specification and the code in the repository are
Apache-2.0: anyone can read and write `.bread` in any program, open or closed, with no fee and no
permission to ask. Breadslicer, the slicer the format came from, is a separate program under the
AGPL; using `.bread` puts none of its terms on anyone.

**Is the specification stable?** Version 1 is frozen (tag `spec-v1`). From here on, the container
and each chunk change only through new versions, never by editing version 1, and a file valid by
version 1 stays valid ([status.md](status.md)). A reader skips the chunks it does not know, so new
chunks do not break old readers.

**Who made it?** The author of Breadslicer and Caliper. A second reader, written from the
specification alone, reads every conformance file the way the reference implementation does
([second-reader.md](second-reader.md)), and Caliper is a third ([caliper.md](caliper.md)).
