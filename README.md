# bread-format

`.bread` is an open file format for 3D printing models: triangle meshes and the scene that places
them, stored small and without losing anything. It is meant to replace STL wherever a model goes
from one program or person to another, and it has room for data that only some programs use, such
as a slicer's editable supports.

**Try it:** [bread.synrgs.tech/viewer](https://bread.synrgs.tech/viewer/) opens `.bread` files and
turns STL into `.bread`, in your browser. Files never leave your computer.

The name: **BREAD** stands for **B**it-exact **R**epresentation of **E**ditable **A**ssembly
**D**ata. Really, it is bread: the format began as the project file of
[Breadslicer](#breadslicer), and a slicer slices the loaf.

## Lossless

A mesh read back from a `.bread` file is the mesh that was written:

- every vertex coordinate, bit for bit;
- every triangle, in the same order, starting at the same corner.

What is not kept: the facet normal that STL stores with every triangle, and its two attribute
bytes. They are file data, not geometry; a program that needs the normal computes it from the
corners, which keep their order. So an STL converted to `.bread` and back has every corner of every
triangle exactly as before, but is not the same file byte for byte.

A `.bread` file holds geometry only: no colours, textures, UV coordinates, materials or animation.

## How small

One figure of 1.9 M triangles, stored seven ways, all of them lossless:

| Stored as | Size |
|---|---|
| Binary STL | 94.9 MB |
| OBJ | 69.7 MB |
| Binary STL in a zip (Deflate, level 9) | 39.3 MB |
| Binary STL in a 7z archive (`-mx=9`) | 32.2 MB |
| OBJ with Zstandard, level 19 | 12.4 MB |
| **`.bread`** | **6.6 MB**, 14.4× smaller than the STL |

On a public corpus, the 2,288 miniatures that MZ4250 shares (162 GB of binary STL, 3.24 billion
triangles), `.bread` came out 14 times smaller overall: from 7.7 to 55.7 times model by model, with
a median of 15.1. Every model came back bit for bit. The models are not redistributed here;
anyone can download them and measure again with [`tools/corpus-bench`](tools/corpus-bench).
Models by MZ4250 ([Printables](https://www.printables.com/@MZ4250), [mz4250.com](https://mz4250.com/)),
under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

How much smaller depends on the model. Over 22 meshes of other kinds (sculptures, 3D scans, CAD
parts and supports), `.bread` was 5.9 to 9.1 times smaller than binary STL.

The figures and how they were measured are in [`web/`](web), which the format page is built from:
every number in [`web/facts.toml`](web/facts.toml) gives its source.

## What a file holds

A file is a short header followed by chunks. Each chunk has a four-byte tag, a version of its own,
and a payload compressed with Zstandard and guarded by a CRC-32. A reader skips the chunks it does
not know, so programs can keep data of their own beside the official chunks.

| Chunk | Holds |
|---|---|
| `SCNE` | The scene: parts, groups and their transforms, names and comments. One mesh can be placed many times. |
| `MESH` | One triangle mesh. |
| `THMB` | A thumbnail, as PNG. |
| `SPRV` | Support previews: meshes that show a slicer's supports to programs that cannot rebuild them. |
| `SURF` | Paint on the faces of models. |
| `PROJ`, `SUPP`, `HOLW` | A slicer's settings, editable supports and hollows. |

## In this repository

| Folder | What |
|---|---|
| [`SPEC.md`](SPEC.md) | The specification, version 1, frozen (tag `spec-v1`). |
| [`conformance/`](conformance) | Valid and invalid files, each with what a reader must make of it. |
| [`crates/bread-format`](crates/bread-format) | The reference implementation, in Rust: reads and writes `.bread`, and reads STL. |
| [`crates/bread-wasm`](crates/bread-wasm) | The same crate built to WebAssembly. |
| [`bindings/js`](bindings/js) | The JavaScript package, for the browser and Node, on that WebAssembly module. |
| [`python/`](python) | A second reader, in plain Python, written from the specification alone. |
| [`viewer/`](viewer) | The viewer and converter at [bread.synrgs.tech/viewer](https://bread.synrgs.tech/viewer/). |
| [`tools/corpus-bench`](tools/corpus-bench) | Measures `.bread` on a folder of STL files. |

None of them is published to a package registry yet. To use the crate, take it from here:

```toml
[dependencies]
bread-format = { git = "https://github.com/breadslicer-app/bread-format" }
```

```rust
let file = bread_format::read_file(std::fs::File::open("model.bread")?)?;
for mesh in &file.scene.meshes {
    // vertices: [f32; 3] each; triangles: three vertex indices each.
    println!("{} vertices, {} triangles", mesh.vertices.len(), mesh.triangles.len());
}
```

The JavaScript package and the Python reader say how to build and use them in their own READMEs.

## Programs that read it

- **[Breadslicer](#breadslicer)**, the slicer the format was made for, keeps its projects as
  `.bread`.
- **[Caliper](https://caliper.synrgs.tech)**, a mesh repair program, opens and saves `.bread` since
  version 0.9.6.
- **The viewer** at [bread.synrgs.tech/viewer](https://bread.synrgs.tech/viewer/).

To write a reader of your own, start from [`SPEC.md`](SPEC.md) and check it against
[`conformance/`](conformance).

## Breadslicer

Breadslicer is a slicer for resin printers, by the same author, still in development. It is a
separate program under a license of its own, the AGPL. This repository holds only the format: the specification, the conformance
files and the implementations here are all under Apache-2.0, and using them puts no obligation of
Breadslicer's on you.

## License

Apache-2.0 ([`LICENSE`](LICENSE)); the Python reader carries its own copy. The crate builds in meshoptimizer (MIT, by Arseny Kapoulkine) and Zstandard (BSD);
their notices are in [`crates/bread-format/NOTICE`](crates/bread-format/NOTICE) and go with every
program built from it.
