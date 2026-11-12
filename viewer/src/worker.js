// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// The viewer's work, off the page's thread so the page never freezes: reading
// .bread files, and turning STL files into one. The reading module loads at
// once; the writing module, more than twice its size, only when STL files
// first come in.

import { BreadError, load } from './bread.js';
import { convertStl, openBread, placedSize } from './core.js';

const reader = load(fetch(new URL('bread.wasm', import.meta.url)));
let writer = null;

/** The error as the page shows it: a reason code when the format gave one. */
function failure(err) {
	if (err instanceof BreadError) return { reason: err.reason, chunk: err.chunk, message: err.message };
	if (err instanceof RangeError) return { reason: 'too-many-leaves', chunk: null, message: err.message };
	return { reason: null, chunk: null, message: String(err?.message ?? err) };
}

/** Every buffer in `meshes`, to hand them over instead of copying them. */
function buffers(meshes) {
	return meshes.flatMap((m) => [m.vertices.buffer, m.triangles.buffer]);
}

async function open(bytes) {
	const started = performance.now();
	const read = openBread(await reader, bytes);
	const ms = performance.now() - started;
	const sizes = read.placements.map((p) => placedSize(read.meshes[p.mesh], p.matrix));
	return [{ ...read, sizes, ms, size: bytes.length }, buffers(read.meshes)];
}

async function convert(files, name) {
	writer ??= load(fetch(new URL('bread-write.wasm', import.meta.url)));
	const started = performance.now();
	const result = convertStl(await writer, files, name === undefined ? {} : { name });
	return [{ ...result, ms: performance.now() - started }, [result.file.buffer]];
}

self.onmessage = async ({ data }) => {
	const { id, type } = data;
	try {
		const [result, transfer] =
			type === 'open' ? await open(data.bytes) : await convert(data.files, data.name);
		self.postMessage({ id, result }, transfer);
	} catch (err) {
		self.postMessage({ id, error: failure(err) });
	}
};
