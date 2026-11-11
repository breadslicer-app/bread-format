# bread-format for JavaScript

Reads `.bread` files in the browser and in Node, through the `bread-format` crate built to
WebAssembly ([`crates/bread-wasm`](../../crates/bread-wasm)): one implementation, with the same
checks as every other reader. It has no dependencies. It is not published yet; the package name
is still to be settled.

So far it reads the container, the chunks of a file, each checked and decompressed, and decodes
the meshes in them, `MESH` versions 1 and 2. Reading the scene, and writing, come next.

```js
import { load, BreadError } from 'bread-format';

const bread = await load(fetch(new URL('bread.wasm', import.meta.url)));
try {
	for (const chunk of bread.readChunks(bytes)) {
		if (chunk.tag !== 'MESH') continue;
		// vertices: Float32Array of x, y, z; triangles: Uint32Array of indices.
		const { meta, vertices, triangles } = bread.decodeMesh(chunk);
		console.log(meta?.name, vertices.length / 3, triangles.length / 3);
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

## Building and testing

`npm run build` builds the module into `dist/bread.wasm` (see [build.mjs](build.mjs) for what it
needs: the `wasm32-unknown-unknown` Rust target, and a clang that targets WebAssembly through
`WASI_SDK_PATH`). `npm test` checks it against the conformance files.

`dist/` also gets the module's [NOTICE](../../crates/bread-wasm/NOTICE), for meshoptimizer and
zstd: keep it with the module wherever it goes.
