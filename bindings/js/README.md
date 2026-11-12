# bread-format for JavaScript

Reads `.bread` files in the browser and in Node, through the `bread-format` crate built to
WebAssembly ([`crates/bread-wasm`](../../crates/bread-wasm)): one implementation, with the same
checks as every other reader. It has no dependencies. It is not published yet; the package name
is still to be settled.

It reads the container, the chunks of a file, each checked and decompressed; the meshes in them,
`MESH` versions 1 and 2; and the scene that places them. It also reads STL files and writes `.bread`
files.

The module comes built twice. `dist/bread.wasm` reads, and is about 73 KB with brotli.
`dist/bread-write.wasm` also reads STL and writes `.bread`, and is about 171 KB, because it takes
zstd's compressor and meshoptimizer's encoders. A page that only shows files loads the first; one
that converts loads the second when it is needed. Both read the same way.

```js
import { load, placeMeshes, BreadError } from 'bread-format';

const bread = await load(fetch(new URL('bread.wasm', import.meta.url)));
try {
	const chunks = bread.readChunks(bytes);
	// vertices: Float32Array of x, y, z; triangles: Uint32Array of indices.
	const meshes = chunks.filter((chunk) => chunk.tag === 'MESH').map((chunk) => bread.decodeMesh(chunk));
	const scene = bread.decodeScene(chunks);
	for (const { leaf, mesh, matrix } of placeMeshes(scene)) {
		// matrix: the leaf's world transform, 4 × 4 in column-major order.
		console.log(scene.nodes[leaf].meta?.name, meshes[mesh].triangles.length / 3, matrix);
	}
} catch (err) {
	if (err instanceof BreadError) console.log(err.reason, err.chunk);
	else throw err;
}
```

`readChunks` returns the chunks this version of the format knows, in file order, and skips the
others, official or private. It throws a `BreadError` for a file it refuses: `reason` is one of the
reason codes of [`conformance/README.md`](../../conformance/README.md), and `chunk` the tag of the
chunk it is about, or `null`.

`decodeMesh` takes a `MESH` chunk as `readChunks` gives it and returns its metadata (`null`, or a
`name` and `comment`, each a string or `null`), its vertices as x, y and z of each, and its
triangles as three vertex indices each, first corner first, ready for a `BufferGeometry`.
Coordinates come back bit for bit. It throws a `BreadError` with reason `malformed`,
`no-such-vertex` or `version` for a mesh it refuses.

`decodeScene` takes the chunks `readChunks` gives and returns the scene ([`SPEC.md`, section
5](../../SPEC.md#5-scene-scne)) as it is stored: its metadata, `meshCount`, `meshResources`,
`nodes`, `subtrees` and `roots`, which name meshes by their order among the `MESH` chunks. Each node
has a `kind` (`'group'`, `'leaf'`, `'shared leaf'` or `'reference'`), its `meta`, its `transform`
(`null` for the identity, or `linear`, three rows, and `translation`), and by kind its `children`,
`mesh`, `resource` or `subtree`. It throws a `BreadError` with reason `chunk-count` when the file
does not hold exactly one scene chunk, and `malformed` or `version` for a scene it refuses.

`placeMeshes` walks a scene as section 5.4 says and returns every leaf it shows: the leaf's node
index, the mesh it shows, and its world transform as a 4 × 4 matrix in column-major order,
computed in `f64`, ready for `Matrix4.fromArray` in three.js. A leaf inside a subtree comes once for
each reference that reaches it. References can copy subtrees that copy subtrees, so a small file can
ask for a great many leaves: past a million nodes walked, or `{ limit }`, it throws a `RangeError`.

### Writing, with `bread-write.wasm`

```js
const writer = await load(fetch(new URL('bread-write.wasm', import.meta.url)));
// Each STL a part named after its file, in one .bread file.
const { file } = writer.fromStl([{ name: 'arm.stl', bytes: armStl }, { name: 'base.stl', bytes: baseStl }]);
```

`readStl` reads a binary or ASCII STL into a mesh, as `decodeMesh` gives one. Two corners are one
vertex when their coordinates are equal bit for bit, numbered by first use, so every corner of every
triangle comes back exactly, in the STL's order. An ASCII STL's solid name becomes the mesh's name,
and a binary header that holds text becomes its comment. It throws a `BreadError` with reason
`not-stl`, `bad-stl` (an ASCII STL that breaks its grammar) or `too-large`.

`writeFile(scene, meshes)` writes a file of meshes, as `decodeMesh` gives them, placed by a scene,
as `decodeScene` gives one. The scene is checked as a reader checks it, so a scene a reader would
refuse throws `malformed` and is never written. Coordinates are written bit for bit, and triangles
in their order. `fromStl(files, { name })` does both for a list of STLs: each becomes a mesh shown
by a leaf named after its file, where the STL put it.

`canWrite` tells the modules apart; the writing methods throw a `TypeError` on `bread.wasm`.

## Building and testing

`npm run build` builds the module into `dist/bread.wasm` (see [build.mjs](build.mjs) for what it
needs: the `wasm32-unknown-unknown` Rust target, and a clang that targets WebAssembly through
`WASI_SDK_PATH`). `npm test` checks it against the conformance files.

`dist/` also gets the module's [NOTICE](../../crates/bread-wasm/NOTICE), for meshoptimizer and
zstd: keep it with the module wherever it goes.
