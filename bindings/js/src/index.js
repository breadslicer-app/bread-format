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
		const input = x.bread_alloc(bytes.length);
		// Memory can grow on any call, so every view is made after it.
		new Uint8Array(x.memory.buffer, input, bytes.length).set(bytes);
		const read = x.bread_read_chunks(input, bytes.length);
		x.bread_free(input, bytes.length);
		try {
			if (!read) {
				const reason = decoder.decode(
					new Uint8Array(x.memory.buffer, x.bread_refusal_reason(), x.bread_refusal_reason_len()),
				);
				const chunk = x.bread_refusal_chunk();
				throw new BreadError(reason, chunk === 0 ? null : tagName(chunk));
			}
			const chunks = [];
			for (let i = 0, n = x.bread_chunk_count(); i < n; i++) {
				const payload = new Uint8Array(
					x.memory.buffer,
					x.bread_chunk_payload(i),
					x.bread_chunk_payload_len(i),
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
}
