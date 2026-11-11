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

/** Where `bread_mesh_text_len` says a mesh has no such text. */
const NO_TEXT = 0xffffffff;

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
			const text = (which) => {
				const len = u32(x.bread_mesh_text_len(which));
				if (len === NO_TEXT) return null;
				return decoder.decode(new Uint8Array(x.memory.buffer, u32(x.bread_mesh_text(which)), len));
			};
			const meta = x.bread_mesh_has_meta() ? { name: text(0), comment: text(1) } : null;
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
