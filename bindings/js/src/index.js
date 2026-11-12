// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Reads .bread files in JavaScript through the bread-format crate built to
// WebAssembly (crates/bread-wasm): the same code and the same checks as every
// other reader.

/** What each reason code of conformance/README.md means. */
const MESSAGES = {
	'not-bread': 'not a .bread file',
	version: 'written in a newer version of the format',
	'cut-short': 'the file is cut short',
	crc: 'a chunk is damaged: its bytes do not match their checksum',
	'bad-payload': 'a chunk does not decompress to its recorded length',
	malformed: 'a chunk is malformed',
	'no-such-vertex': 'a triangle names a vertex its mesh does not have',
	'chunk-count': 'the file holds more or fewer chunks of a kind than the format allows',
	'not-stl': 'not an STL file',
	'bad-stl': 'the ASCII STL is malformed',
	'too-large': 'too large for a .bread file',
};

/** A file the reader refused. */
export class BreadError extends Error {
	/**
	 * @param {string} reason a reason code of conformance/README.md
	 * @param {string | null} chunk the tag of the chunk it is about, if any
	 */
	constructor(reason, chunk) {
		const message = MESSAGES[reason] ?? reason;
		super(chunk === null ? message : `${message} (${chunk} chunk)`);
		this.name = 'BreadError';
		this.reason = reason;
		this.chunk = chunk;
	}
}

/**
 * Loads the WebAssembly module, bread.wasm.
 *
 * @param {BufferSource | WebAssembly.Module | Response | PromiseLike<Response>} source
 *   its bytes, the compiled module, or the response of fetching it, such as
 *   `fetch(new URL('bread.wasm', import.meta.url))`
 * @returns {Promise<Bread>}
 */
export async function load(source) {
	const resolved = await source;
	let instance;
	if (resolved instanceof WebAssembly.Module) {
		instance = await WebAssembly.instantiate(resolved, {});
	} else if (typeof Response !== 'undefined' && resolved instanceof Response) {
		({ instance } = await WebAssembly.instantiateStreaming(resolved, {}));
	} else {
		({ instance } = await WebAssembly.instantiate(resolved, {}));
	}
	return new Bread(instance.exports);
}

const decoder = new TextDecoder();
const encoder = new TextEncoder();

/**
 * A scene as a scene chunk payload (SPEC.md 5.1) over `meshCount` meshes.
 * Each subtree record goes right after its root, or after the subtree before
 * it when that comes later, as the reference implementation writes them. The
 * module checks the payload as a reader does before it writes it.
 *
 * @param {Scene} scene
 * @param {number} meshCount
 * @returns {Uint8Array}
 */
function encodeScene(scene, meshCount) {
	const out = [];
	const u8 = (v) => out.push(v);
	const u32v = (v) => {
		if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new TypeError(`not an index: ${v}`);
		out.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, v >>> 24);
	};
	const text = (s) => {
		if (s === null || s === undefined) return u8(0);
		const bytes = encoder.encode(s);
		u8(1);
		u32v(bytes.length);
		for (const b of bytes) out.push(b);
	};
	const meta = (m) => {
		if (m === null || m === undefined) return u8(0);
		u8(1);
		text(m.name);
		text(m.comment);
	};
	const f64 = new DataView(new ArrayBuffer(8));
	const transform = (t) => {
		if (t === null || t === undefined) return u8(0);
		u8(1);
		for (const v of [...t.linear.flat(), ...t.translation]) {
			f64.setFloat64(0, v, true);
			for (let i = 0; i < 8; i++) out.push(f64.getUint8(i));
		}
	};
	const list = (values) => {
		u32v(values.length);
		for (const v of values) u32v(v);
	};
	const KIND = { group: 0, leaf: 1, 'shared leaf': 2, reference: 4 };
	const subtrees = scene.subtrees ?? [];

	meta(scene.meta);
	u32v(meshCount);
	list(scene.meshResources ?? []);
	u32v(scene.nodes.length + subtrees.length);
	let next = 0;
	for (const [listed, node] of scene.nodes.entries()) {
		const kind = KIND[node.kind];
		if (kind === undefined) throw new TypeError(`not a node kind: ${node.kind}`);
		u8(kind);
		meta(node.meta);
		transform(node.transform);
		if (node.kind === 'group') list(node.children);
		else u32v(node.kind === 'leaf' ? node.mesh : node.kind === 'shared leaf' ? node.resource : node.subtree);
		while (next < subtrees.length && subtrees[next] <= listed) {
			u8(5);
			u32v(subtrees[next++]);
		}
	}
	// Subtrees whose roots are not nodes: the module refuses the payload.
	while (next < subtrees.length) {
		u8(5);
		u32v(subtrees[next++]);
	}
	list(scene.roots);
	return Uint8Array.from(out);
}

/** Where `bread_mesh_text_len` says a mesh has no such text. */
const NO_TEXT = 0xffffffff;

/** What `bread_scene_has_meta` and `bread_scene_text` call the scene itself. */
const SCENE_ITSELF = -1;

/** Node kinds, by the number their record has. */
const NODE_KINDS = { 0: 'group', 1: 'leaf', 2: 'shared leaf', 4: 'reference' };

/**
 * @typedef {{ name: string | null, comment: string | null } | null} Meta
 * @typedef {{ linear: number[][], translation: number[] } | null} Transform
 * @typedef {{
 *   meta: Meta,
 *   meshCount: number,
 *   meshResources: number[],
 *   nodes: ({ kind: string, meta: Meta, transform: Transform, children?: number[],
 *     mesh?: number, resource?: number, subtree?: number })[],
 *   subtrees: number[],
 *   roots: number[],
 * }} Scene
 */

/** How many nodes `placeMeshes` walks at most, unless told otherwise. */
const PLACEMENT_LIMIT = 1_000_000;

/**
 * Every leaf the scene shows, with its mesh and its world transform, in the
 * order of the walk of SPEC.md 5.4: down from each root in order, each node's
 * transform after its parent's, and a reference going on into its subtree's
 * root as if that were its only child. A leaf inside a subtree comes once for
 * every reference that reaches it. Nodes with no place, and subtrees no
 * reference names, are not shown.
 *
 * `matrix` is the world transform as a 4 × 4 matrix in column-major order,
 * as WebGL and `Matrix4.fromArray` of three.js take it, computed in f64.
 *
 * References can copy subtrees that themselves copy subtrees, so a small file
 * can ask for a great many: past `limit` nodes walked, each copy counted, this
 * throws a `RangeError` instead of walking on.
 *
 * @param {Scene} scene as `decodeScene` gives it
 * @param {{ limit?: number }} [options]
 * @returns {{ leaf: number, mesh: number, matrix: Float64Array }[]}
 */
export function placeMeshes(scene, { limit = PLACEMENT_LIMIT } = {}) {
	const placed = [];
	// Each transform as [m00, m01, m02, tx, m10, ..., tz]: three rows of four.
	const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0];
	const own = (node) => {
		const t = node.transform;
		if (t === null) return IDENTITY;
		const [r0, r1, r2] = t.linear;
		const [tx, ty, tz] = t.translation;
		return [...r0, tx, ...r1, ty, ...r2, tz];
	};
	// a after b: p goes to a(b(p)).
	const compose = (a, b) => {
		const out = new Array(12);
		for (let r = 0; r < 3; r++) {
			for (let c = 0; c < 4; c++) {
				let sum = a[4 * r] * b[c] + a[4 * r + 1] * b[4 + c] + a[4 * r + 2] * b[8 + c];
				if (c === 3) sum += a[4 * r + 3];
				out[4 * r + c] = sum;
			}
		}
		return out;
	};
	// Walked with a stack, not recursion: a deep tree cannot overflow it.
	const stack = [];
	for (let i = scene.roots.length - 1; i >= 0; i--) stack.push([scene.roots[i], IDENTITY]);
	let walked = 0;
	while (stack.length > 0) {
		if (++walked > limit) throw new RangeError(`the scene walks more than ${limit} nodes`);
		const [index, parent] = stack.pop();
		const node = scene.nodes[index];
		const world = node.transform === null ? parent : compose(parent, own(node));
		if (node.kind === 'group') {
			for (let i = node.children.length - 1; i >= 0; i--) stack.push([node.children[i], world]);
		} else if (node.kind === 'reference') {
			stack.push([scene.subtrees[node.subtree], world]);
		} else {
			const mesh = node.kind === 'leaf' ? node.mesh : scene.meshResources[node.resource];
			const w = world;
			placed.push({
				leaf: index,
				mesh,
				matrix: Float64Array.of(w[0], w[4], w[8], 0, w[1], w[5], w[9], 0, w[2], w[6], w[10], 0, w[3], w[7], w[11], 1),
			});
		}
	}
	return placed;
}

/**
 * A pointer or length from the module, which WebAssembly hands JavaScript as
 * a signed 32-bit number: read it unsigned.
 */
const u32 = (n) => n >>> 0;

/** Four tag bytes, as the module gives them, to a string such as "MESH". */
function tagName(tag) {
	return String.fromCharCode(tag & 0xff, (tag >>> 8) & 0xff, (tag >>> 16) & 0xff, tag >>> 24);
}

/** A loaded module. */
export class Bread {
	#x;

	constructor(exports) {
		this.#x = exports;
	}

	/**
	 * The chunks of a file that this version of the format knows, in file
	 * order, each checked and decompressed. Unknown chunks, official or
	 * private, are skipped. Older supports and hollows chunks are kept, at
	 * their version, for the reader to skip.
	 *
	 * @param {Uint8Array} bytes the whole file
	 * @returns {{ tag: string, version: number, payload: Uint8Array }[]}
	 * @throws {BreadError} when the file is refused
	 */
	readChunks(bytes) {
		const x = this.#x;
		const input = u32(x.bread_alloc(bytes.length));
		// Memory can grow on any call, so every view is made after it.
		new Uint8Array(x.memory.buffer, input, bytes.length).set(bytes);
		const read = x.bread_read_chunks(input, bytes.length);
		x.bread_free(input, bytes.length);
		try {
			if (!read) throw this.#refusal();
			const chunks = [];
			for (let i = 0, n = u32(x.bread_chunk_count()); i < n; i++) {
				const payload = new Uint8Array(
					x.memory.buffer,
					u32(x.bread_chunk_payload(i)),
					u32(x.bread_chunk_payload_len(i)),
				);
				chunks.push({
					tag: tagName(x.bread_chunk_tag(i)),
					version: x.bread_chunk_version(i),
					payload: payload.slice(),
				});
			}
			return chunks;
		} finally {
			x.bread_release();
		}
	}

	/**
	 * The mesh of a mesh chunk, as `readChunks` gives it: its vertices as x,
	 * y and z of each, and its triangles as three vertex indices each, first
	 * corner first, ready for a `BufferGeometry`. Coordinates come back bit
	 * for bit, and triangles in their order.
	 *
	 * @param {{ tag?: string, version: number, payload: Uint8Array }} chunk
	 * @returns {{
	 *   meta: { name: string | null, comment: string | null } | null,
	 *   vertices: Float32Array,
	 *   triangles: Uint32Array,
	 * }}
	 * @throws {BreadError} when the mesh is refused
	 */
	decodeMesh(chunk) {
		if (chunk.tag !== undefined && chunk.tag !== 'MESH') {
			throw new TypeError(`not a mesh chunk: ${chunk.tag}`);
		}
		const x = this.#x;
		const bytes = chunk.payload;
		const input = u32(x.bread_alloc(bytes.length));
		new Uint8Array(x.memory.buffer, input, bytes.length).set(bytes);
		const read = x.bread_decode_mesh(chunk.version, input, bytes.length);
		x.bread_free(input, bytes.length);
		try {
			if (!read) throw this.#refusal();
			return this.#lastMesh();
		} finally {
			x.bread_release();
		}
	}

	/** The mesh the module last decoded or read, copied out. */
	#lastMesh() {
		const x = this.#x;
		const text = (which) => {
			const len = u32(x.bread_mesh_text_len(which));
			if (len === NO_TEXT) return null;
			return decoder.decode(new Uint8Array(x.memory.buffer, u32(x.bread_mesh_text(which)), len));
		};
		const meta = x.bread_mesh_has_meta() ? { name: text(0), comment: text(1) } : null;
		// slice() copies the bytes, so every coordinate keeps its bits.
		const vertices = new Float32Array(
			x.memory.buffer,
			u32(x.bread_mesh_vertices()),
			3 * u32(x.bread_mesh_vertex_count()),
		).slice();
		const triangles = new Uint32Array(
			x.memory.buffer,
			u32(x.bread_mesh_triangles()),
			3 * u32(x.bread_mesh_triangle_count()),
		).slice();
		return { meta, vertices, triangles };
	}

	/** Whether this module writes: `bread-write.wasm` does, `bread.wasm` does not. */
	get canWrite() {
		return typeof this.#x.bread_write_begin === 'function';
	}

	#needWriter() {
		if (!this.canWrite) throw new TypeError('this module only reads: load bread-write.wasm to write');
	}

	/**
	 * Copies `bytes` into the module, calls `f` with where they are and how
	 * many, and frees them. Even no bytes get a real pointer, as Rust needs.
	 */
	#withInput(bytes, f) {
		const x = this.#x;
		const at = u32(x.bread_alloc(bytes.length));
		new Uint8Array(x.memory.buffer, at, bytes.length).set(bytes);
		try {
			return f(at, bytes.length);
		} finally {
			x.bread_free(at, bytes.length);
		}
	}

	/**
	 * An STL file, binary or ASCII, read into a mesh as `decodeMesh` gives
	 * one. Two corners are one vertex when their coordinates are equal bit for
	 * bit, numbered by first use, so every corner of every triangle comes back
	 * exactly, in the STL's order. An ASCII STL's solid name is the mesh's
	 * name, and a binary header that holds text its comment. Needs
	 * `bread-write.wasm`.
	 *
	 * @param {Uint8Array} bytes the whole STL
	 * @returns {{ meta: Meta, vertices: Float32Array, triangles: Uint32Array }}
	 * @throws {BreadError} `not-stl`, `bad-stl` or `too-large`
	 */
	readStl(bytes) {
		this.#needWriter();
		const x = this.#x;
		const read = this.#withInput(bytes, (at, len) => x.bread_read_stl(at, len));
		try {
			if (!read) throw this.#refusal();
			return this.#lastMesh();
		} finally {
			x.bread_release();
		}
	}

	/**
	 * A `.bread` file of `meshes`, placed by `scene`: a scene as `decodeScene`
	 * gives one, whose `meshCount`, if given, is ignored. The scene is
	 * checked as a reader checks it, so a scene a reader would refuse is
	 * refused here and never written. Coordinates are written bit for bit and
	 * triangles in their order. Needs `bread-write.wasm`.
	 *
	 * @param {Scene} scene
	 * @param {{ meta: Meta, vertices: Float32Array, triangles: Uint32Array }[]} meshes
	 * @returns {Uint8Array} the file
	 * @throws {BreadError} `malformed` for a scene or mesh it refuses,
	 *   `no-such-vertex`, or `too-large`
	 */
	writeFile(scene, meshes) {
		this.#needWriter();
		const x = this.#x;
		const utf8 = (s) => (s === null || s === undefined ? null : encoder.encode(s));
		const bytesOf = (array) => new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
		try {
			x.bread_write_begin();
			for (const mesh of meshes) {
				const meta = mesh.meta ?? null;
				const name = meta === null ? null : utf8(meta.name);
				const comment = meta === null ? null : utf8(meta.comment);
				// A text pointer of 0 is no text; an empty text still needs one.
				const added = this.#withInput(bytesOf(mesh.vertices), (v, vLen) =>
					this.#withInput(bytesOf(mesh.triangles), (t, tLen) =>
						this.#withText(name, (n, nLen) =>
							this.#withText(comment, (c, cLen) =>
								x.bread_write_mesh(v, vLen, t, tLen, meta === null ? 0 : 1, n, nLen, c, cLen),
							),
						),
					),
				);
				if (!added) throw this.#refusal();
			}
			const payload = encodeScene(scene, meshes.length);
			const written = this.#withInput(payload, (at, len) => x.bread_write_file(at, len));
			if (!written) throw this.#refusal();
			return new Uint8Array(x.memory.buffer, u32(x.bread_output()), u32(x.bread_output_len())).slice();
		} finally {
			x.bread_release();
		}
	}

	/** As `#withInput`, but `null` is no text, given as pointer 0. */
	#withText(bytes, f) {
		if (bytes === null) return f(0, 0);
		return this.#withInput(bytes, f);
	}

	/**
	 * STL files as one `.bread` file: each STL a mesh, shown by a leaf named
	 * after its file, in order, with no transform: they stay where the STLs
	 * put them. Several STLs make a kit of parts in one file. Needs
	 * `bread-write.wasm`.
	 *
	 * @param {{ name: string, bytes: Uint8Array }[]} files
	 * @param {{ name?: string }} [options] a name for the scene
	 * @returns {{ file: Uint8Array, meshes: { meta: Meta, vertices: Float32Array, triangles: Uint32Array }[] }}
	 *   the `.bread` file, and the meshes read from the STLs
	 * @throws {BreadError} for an STL it cannot read, as `readStl`
	 */
	fromStl(files, { name } = {}) {
		const meshes = files.map((file) => this.readStl(file.bytes));
		const scene = {
			meta: name === undefined ? null : { name, comment: null },
			meshResources: [],
			nodes: files.map((file, mesh) => ({
				kind: 'leaf',
				meta: { name: file.name, comment: null },
				transform: null,
				mesh,
			})),
			subtrees: [],
			roots: files.map((_, i) => i),
		};
		return { file: this.writeFile(scene, meshes), meshes };
	}

	/**
	 * The scene of a file, from the chunks `readChunks` gives: its tree of
	 * nodes, which names meshes by index, in the order of the file's `MESH`
	 * chunks. Each node is
	 *
	 * - `{ kind: 'group', meta, transform, children }`, with its child node
	 *   indices in order;
	 * - `{ kind: 'leaf', meta, transform, mesh }`, a mesh index;
	 * - `{ kind: 'shared leaf', meta, transform, resource }`, an index into
	 *   `meshResources`, which gives the mesh;
	 * - `{ kind: 'reference', meta, transform, subtree }`, an index into
	 *   `subtrees`, which gives the subtree's root node.
	 *
	 * A transform is `null` for a node stored without one, the identity, or
	 * `{ linear, translation }`: the 3 × 3 matrix as three rows, and the
	 * translation, taking a point p to linear · p + translation. To draw the
	 * scene, `placeMeshes` walks it.
	 *
	 * @param {{ tag: string, version: number, payload: Uint8Array }[]} chunks
	 * @returns {Scene}
	 * @throws {BreadError} when the scene is refused, or the file does not
	 *   hold exactly one scene chunk
	 */
	decodeScene(chunks) {
		const scenes = chunks.filter((chunk) => chunk.tag === 'SCNE');
		if (scenes.length !== 1) throw new BreadError('chunk-count', 'SCNE');
		const meshCount = chunks.filter((chunk) => chunk.tag === 'MESH').length;
		const x = this.#x;
		const bytes = scenes[0].payload;
		const input = u32(x.bread_alloc(bytes.length));
		new Uint8Array(x.memory.buffer, input, bytes.length).set(bytes);
		const read = x.bread_decode_scene(scenes[0].version, meshCount, input, bytes.length);
		x.bread_free(input, bytes.length);
		try {
			if (!read) throw this.#refusal();
			const indices = (ptr, len) => Array.from(new Uint32Array(x.memory.buffer, u32(ptr), u32(len)));
			const list = (which) => indices(x.bread_scene_list(which), x.bread_scene_list_len(which));
			const meta = (target) => {
				if (!x.bread_scene_has_meta(target)) return null;
				const text = (which) => {
					const len = u32(x.bread_scene_text_len(target, which));
					if (len === NO_TEXT) return null;
					return decoder.decode(new Uint8Array(x.memory.buffer, u32(x.bread_scene_text(target, which)), len));
				};
				return { name: text(0), comment: text(1) };
			};
			const nodes = [];
			for (let i = 0, n = u32(x.bread_scene_node_count()); i < n; i++) {
				const at = u32(x.bread_scene_node_transform(i));
				let transform = null;
				if (at !== 0) {
					const f = new Float64Array(x.memory.buffer, at, 12);
					transform = {
						linear: [
							[f[0], f[1], f[2]],
							[f[3], f[4], f[5]],
							[f[6], f[7], f[8]],
						],
						translation: [f[9], f[10], f[11]],
					};
				}
				const node = { kind: NODE_KINDS[x.bread_scene_node_kind(i)], meta: meta(i), transform };
				const target = u32(x.bread_scene_node_target(i));
				switch (node.kind) {
					case 'group':
						node.children = indices(x.bread_scene_node_children(i), x.bread_scene_node_children_len(i));
						break;
					case 'leaf':
						node.mesh = target;
						break;
					case 'shared leaf':
						node.resource = target;
						break;
					default:
						node.subtree = target;
				}
				nodes.push(node);
			}
			return {
				meta: meta(SCENE_ITSELF),
				meshCount,
				meshResources: list(0),
				nodes,
				subtrees: list(1),
				roots: list(2),
			};
		} finally {
			x.bread_release();
		}
	}

	/** The last refusal, as a `BreadError`. */
	#refusal() {
		const x = this.#x;
		const reason = decoder.decode(
			new Uint8Array(x.memory.buffer, u32(x.bread_refusal_reason()), u32(x.bread_refusal_reason_len())),
		);
		const chunk = x.bread_refusal_chunk();
		return new BreadError(reason, chunk === 0 ? null : tagName(chunk));
	}
}
