// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// decodeMesh against the conformance files. Run `npm run build` first.

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
if (!existsSync(wasm)) {
	throw new Error('dist/bread.wasm is missing: run `npm run build` first');
}
const bread = await load(readFileSync(wasm));

function cases(dir) {
	return readdirSync(join(conformance, dir))
		.filter((name) => name.endsWith('.bread'))
		.map((name) => name.slice(0, -'.bread'.length))
		.sort()
		.map((name) => ({
			name,
			bytes: new Uint8Array(readFileSync(join(conformance, dir, `${name}.bread`))),
			json: JSON.parse(readFileSync(join(conformance, dir, `${name}.json`), 'utf8')),
		}));
}

/** SHA-256 of a typed array's bytes, little-endian as the JSON hashes them. */
function sha256(array) {
	assert.equal(new Uint8Array(new Uint16Array([1]).buffer)[0], 1, 'a little-endian machine');
	return createHash('sha256')
		.update(new Uint8Array(array.buffer, array.byteOffset, array.byteLength))
		.digest('hex');
}

const meshChunks = (bytes) => bread.readChunks(bytes).filter((chunk) => chunk.tag === 'MESH');

for (const { name, bytes, json } of cases('valid')) {
	test(`valid/${name}: its meshes`, () => {
		const chunks = meshChunks(bytes);
		assert.equal(chunks.length, json.meshes.length);
		chunks.forEach((chunk, i) => {
			const mesh = bread.decodeMesh(chunk);
			const want = json.meshes[i];
			assert.deepEqual(mesh.meta, want.meta, `mesh ${i}`);
			assert.equal(mesh.vertices.length, 3 * want.vertex_count, `mesh ${i}`);
			assert.equal(mesh.triangles.length, 3 * want.triangle_count, `mesh ${i}`);
			assert.equal(sha256(mesh.vertices), want.vertices_sha256, `mesh ${i}`);
			assert.equal(sha256(mesh.triangles), want.triangles_sha256, `mesh ${i}`);
		});
	});
}

let refused = 0;
for (const { name, bytes, json } of cases('invalid')) {
	if (json.chunk !== 'MESH' || !['malformed', 'no-such-vertex'].includes(json.reason)) continue;
	refused++;
	test(`invalid/${name}: its mesh is refused for ${json.reason}`, () => {
		const errors = [];
		for (const chunk of meshChunks(bytes)) {
			try {
				bread.decodeMesh(chunk);
			} catch (err) {
				errors.push(err);
			}
		}
		assert.equal(errors.length, 1);
		assert.ok(errors[0] instanceof BreadError);
		assert.equal(errors[0].reason, json.reason);
		assert.equal(errors[0].chunk, 'MESH');
	});
}

test('every mesh case is checked', () => assert.equal(refused, 7));

test('a chunk that is not a mesh is not decoded', () => {
	const triangle = new Uint8Array(readFileSync(join(conformance, 'valid/triangle.bread')));
	const scene = bread.readChunks(triangle).find((chunk) => chunk.tag === 'SCNE');
	assert.throws(() => bread.decodeMesh(scene), TypeError);
});

test('an unknown mesh version is refused', () => {
	const triangle = new Uint8Array(readFileSync(join(conformance, 'valid/triangle.bread')));
	const [mesh] = meshChunks(triangle);
	assert.throws(() => bread.decodeMesh({ ...mesh, version: 3 }), { reason: 'version' });
	// The module is ready for the next mesh after a refusal.
	assert.equal(bread.decodeMesh(mesh).triangles.length, 3);
});
