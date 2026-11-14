---
title: For implementers
status: stable
updated: 2026-11-13
---

Everything needed to read and write `.bread` is in the repository,
https://github.com/breadslicer-app/bread-format, under Apache-2.0:

- **the specification**, `SPEC.md`, version 1, frozen;
- **the conformance files**, `conformance/`: valid files with what a reader must make of each, and
  invalid ones with the reason a reader must give for refusing them ([conformance.md](conformance.md));
- **the reference implementation**, in Rust, also built to WebAssembly for JavaScript.

**Reading, in Rust.** The crate is not on crates.io yet; it is taken from the repository:

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

`bread_format::read_stl` reads an STL into a mesh, and `write_file` writes a file.

**Reading, in JavaScript**, in the browser or Node, through the WebAssembly module (built from
`bindings/js`, not on npm yet):

```js
import { load, placeMeshes } from 'bread-format';

const bread = await load(fetch(new URL('bread.wasm', import.meta.url)));
const chunks = bread.readChunks(bytes);
const meshes = chunks.filter((c) => c.tag === 'MESH').map((c) => bread.decodeMesh(c));
for (const { mesh, matrix } of placeMeshes(bread.decodeScene(chunks))) {
	// meshes[mesh].vertices: Float32Array; .triangles: Uint32Array; matrix: 4 × 4, column-major.
}
```

The module that also writes, `bread-write.wasm`, turns STL files into a `.bread` file with
`fromStl`.

**What exists:**

| | Reads | Writes |
|---|---|---|
| Rust, the reference implementation | yes | yes |
| JavaScript, on WebAssembly | yes | yes |
| Python, written from the specification alone | yes | not yet |
| Caliper | yes | yes |
| A command-line converter | not yet | not yet |
| Reading OBJ into `.bread` | | not yet |

**How to help.** The issues marked *good first issue* and *help wanted* are open to anyone:
reading OBJ in the crate, a command-line converter, and writing from Python. A reader in another
language, written from `SPEC.md` and checked against `conformance/`, is welcome too; differences it
finds between the specification and the reference implementation are bugs in one of them, and
worth an issue.

**Chunks of your own.** A program can keep data of its own in a private chunk, with a lowercase tag
(`SPEC.md`, section 7.2). Other readers skip it and still read the same meshes in the same scene.
