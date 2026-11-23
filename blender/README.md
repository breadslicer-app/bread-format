# .bread for Blender

A Blender extension that opens `.bread` files and saves them.

## Import

File › Import › Bread (.bread), or drop files on the 3D Viewport or the Outliner. A dialog asks
what to bring, all of it ticked:

- **Mesh:** the models, bit for bit: the same vertex coordinates and the same triangles, in their
  order. Leaves that share a mesh share it in Blender too.
- **Hollow:** hollowed models come hollow, as they print. Unticked, they come solid, from the
  original the file keeps ([`HOLW`](../SPEC.md#holw-hollowed-models)).
- **Supports:** rebuilt from the sizes and places the file keeps
  ([`SUPP`](../SPEC.md#supp-supports)). Without a supports document it can read, it shows the
  support preview ([`SPRV`](../SPEC.md#9-support-preview-sprv)), when the file has one. **Detail**
  sets how round they are.

Lengths in a `.bread` file are millimetres; **Scale** gives Blender units per millimetre, 1 by
default, as Blender's STL importer.

## Export

File › Export › Bread (.bread) saves the objects of the scene, or only the selected ones, as one
`.bread` file:

- **Parts:** each object is a named part, in its place. Faces are saved as triangles; a mesh made
  only of triangles keeps them in their order, so a file imported and exported again comes back
  with the same vertex bits and triangles. Objects that show the same mesh share it in the file.
- **Apply Modifiers:** each object as it shows, with its modifiers. An object without modifiers or
  shape keys is saved bit for bit either way. Curves, surfaces and text are saved when this is on.
- **Scale:** Blender units per millimetre, as on import: 1 by default, 0.001 for a scene in metres.
- **Credit:** a title, the designer, the license (an SPDX expression, such as `CC-BY-4.0`) and a
  link to the source go in the file's metadata ([`META`](../SPEC.md#12-metadata-meta)), with the
  date and the program that wrote it. They travel with every copy, and programs that open the file
  can show them. Each one left empty is left out.

Supports and hollows are not written back as such: an imported support comes back as a model.

It needs Blender 4.2 or later, and has been tried on Blender 5.2.

## How it works

- `bread_import/native.py` reads and writes files with the `bread-format` crate, built as a
  native library from [`crates/bread-wasm`](../crates/bread-wasm) with its `write` feature: the C
  ABI the JavaScript package calls, here called through `ctypes`. Every check of the format comes
  with it, and the meshes are written as the crate writes them, `MESH` version 2.
- `bread_import/contents.py` places the scene ([SPEC 5.4](../SPEC.md#54-placing-the-meshes)) and
  reads which leaves are models, hollows and supports.
- `bread_import/supports.py` builds the supports. Breadslicer's supports document (generator `df`,
  versions 1 and 2) already says where each piece goes and how big it is, so nothing here decides
  where supports go: each base becomes a disk with a cone up to its trunk, or, on a raft, a small
foot a little wider than a line of a line raft, as Breadslicer draws it; each stretch between
  joints a cylinder; each joint and knot a ball; each tip a thin disk on the model and a cone to
  the support; each brace a cylinder from knot to knot; and the raft, when there is one, a plate
  under the bases, sloped underneath, with a low wall along its edge that has gaps for resin to
  drain. With one raft for every model, Breadslicer keeps that raft as a mesh of the scene named
  "Raft": it comes with the supports, as it is, and the supports build none of their own. Pieces
  overlap and are not merged, as a slicer expects. They are built with numpy, in batches: about
  0.06 s for a figure with 600 supports.
- `bread_import/importer.py` is the import operator, the menu entry and the file handler for
  dropped files.
- `bread_import/writing.py` turns the parts into a scene chunk and the credit into a metadata
  chunk, which the crate checks as a reader does before it writes the file, and
  `bread_import/exporter.py` is the export operator and its menu entry.

## Build

Needs Rust and Blender:

```
python blender/build.py            # blender/dist/bread_import-<version>.zip
python blender/build.py --install  # and installs and enables it in Blender
```

`BLENDER` names Blender's executable when it is not in the default place. The library is built
for the platform the script runs on; the zip so far is for Windows (`platforms` in
`bread_import/blender_manifest.toml`).

## Tests

```
python -m unittest discover blender/tests -p "test_supports.py"
blender -b --factory-startup --python-exit-code 1 --python blender/tests/test_import.py
blender -b --factory-startup --python-exit-code 1 --python blender/tests/test_export.py
```

The first needs numpy, not Blender. The second, after `build.py`, imports every valid
conformance file and checks that each mesh arrives bit for bit. The third exports each of them
again and imports the result, which comes back with the same mesh bits and triangles in the same
places, and checks shared meshes, triangulation, modifiers, the selection, the scale and the
credit.

## License

The extension is GPL-3.0-or-later, as Blender asks of its extensions (`bread_import/LICENSE`).
The library inside it is the Apache-2.0 `bread-format` crate, with meshoptimizer and Zstandard
(`bread_import/lib/NOTICE`).
