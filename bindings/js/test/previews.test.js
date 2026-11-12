// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// decodePreviews against the conformance files. Run `npm run build` first.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { BreadError, load } from '../src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
const conformance = join(here, '../../../conformance');
const wasm = join(here, '../dist/bread.wasm');
if (!existsSync(wasm)) throw new Error('dist/bread.wasm is missing: run `npm run build` first');
const bread = await load(readFileSync(wasm));

const sha256 = (array) =>
	createHash('sha256').update(new Uint8Array(array.buffer, array.byteOffset, array.byteLength)).digest('hex');

function read(name) {
	const bytes = new Uint8Array(readFileSync(join(conformance, 'valid', name)));
	const chunks = bread.readChunks(bytes);
	const meshes = chunks.filter((c) => c.tag === 'MESH').map((c) => bread.decodeMesh(c));
	return { chunks, meshes };
}

for (const name of readdirSync(join(conformance, 'valid')).filter((n) => n.endsWith('.bread'))) {
	test(`valid/${name}: its support previews`, () => {
		const json = JSON.parse(readFileSync(join(conformance, 'valid', name.replace(/\.bread$/, '.json')), 'utf8'));
		const { chunks, meshes } = read(name);
		const previews = bread.decodePreviews(chunks, meshes);
		assert.deepEqual(
			previews.map((p) => ({
				leaf: p.leaf,
				mesh: {
					meta: p.mesh.meta,
					vertex_count: p.mesh.vertices.length / 3,
					triangle_count: p.mesh.triangles.length / 3,
					vertices_sha256: sha256(p.mesh.vertices),
					triangles_sha256: sha256(p.mesh.triangles),
				},
			})),
			json.support_previews,
		);
	});
}

test('a preview of a leaf that shows a mesh is refused', () => {
	const { chunks, meshes } = read('support-preview.bread');
	const filled = meshes.map(() => ({ vertices: new Float32Array(3), triangles: new Uint32Array() }));
	assert.throws(
		() => bread.decodePreviews(chunks, filled),
		(err) => err instanceof BreadError && err.reason === 'malformed' && err.chunk === 'SPRV',
	);
});

test('two preview chunks are refused', () => {
	const { chunks, meshes } = read('support-preview.bread');
	const twice = [...chunks, chunks.find((c) => c.tag === 'SPRV')];
	assert.throws(
		() => bread.decodePreviews(twice, meshes),
		(err) => err instanceof BreadError && err.reason === 'chunk-count' && err.chunk === 'SPRV',
	);
});
