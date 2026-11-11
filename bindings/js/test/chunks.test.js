// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// readChunks against the conformance files. Run `npm run build` first.

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

/** The reasons a container reader gives on its own. */
const CONTAINER_REASONS = ['not-bread', 'version', 'cut-short', 'crc', 'bad-payload'];
const KNOWN = ['THMB', 'SCNE', 'PROJ', 'SUPP', 'HOLW', 'SURF', 'MESH', 'SPRV'];

function cases(dir) {
	const names = readdirSync(join(conformance, dir))
		.filter((name) => name.endsWith('.bread'))
		.map((name) => name.slice(0, -'.bread'.length))
		.sort();
	assert.ok(names.length > 0, `no files in ${dir}`);
	return names.map((name) => ({
		name,
		bytes: new Uint8Array(readFileSync(join(conformance, dir, `${name}.bread`))),
		json: JSON.parse(readFileSync(join(conformance, dir, `${name}.json`), 'utf8')),
	}));
}

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

/** A payload carried whole, as the JSON gives it: `null`, or its size and hash. */
function carried(chunks, tag) {
	const found = chunks.filter((chunk) => chunk.tag === tag);
	assert.ok(found.length <= 1, `two ${tag} chunks`);
	return found.length === 0 ? null : { size: found[0].payload.length, sha256: sha256(found[0].payload) };
}

for (const { name, bytes, json } of cases('valid')) {
	test(`valid/${name}: its chunks`, () => {
		const chunks = bread.readChunks(bytes);
		const count = (tag) => chunks.filter((chunk) => chunk.tag === tag).length;
		for (const chunk of chunks) assert.ok(KNOWN.includes(chunk.tag), `unknown ${chunk.tag} returned`);
		assert.equal(count('SCNE'), 1);
		assert.equal(count('MESH'), json.meshes.length);
		assert.equal(count('SUPP'), json.supports.length > 0 ? 1 : 0);
		assert.equal(count('HOLW'), json.hollows.length > 0 ? 1 : 0);
		assert.equal(count('SPRV'), json.support_previews.length > 0 ? 1 : 0);
		assert.equal(count('SURF'), json.surface.length > 0 ? 1 : 0);
		assert.deepEqual(carried(chunks, 'PROJ'), json.settings);
		assert.deepEqual(carried(chunks, 'THMB'), json.thumbnail);
	});
}

test('unknown and private chunks are skipped', () => {
	for (const { name, bytes } of cases('valid')) {
		if (name === 'unknown-chunk' || name === 'private-chunk') {
			assert.deepEqual(
				bread.readChunks(bytes).map((chunk) => chunk.tag),
				['SCNE', 'MESH'],
				name,
			);
		}
	}
});

let refused = 0;
for (const { name, bytes, json } of cases('invalid')) {
	if (CONTAINER_REASONS.includes(json.reason)) {
		refused++;
		test(`invalid/${name}: refused for ${json.reason}`, () => {
			assert.throws(
				() => bread.readChunks(bytes),
				(err) => err instanceof BreadError && err.reason === json.reason && err.chunk === json.chunk,
			);
		});
	} else {
		test(`invalid/${name}: left to the chunk readers`, () => {
			assert.doesNotThrow(() => bread.readChunks(bytes));
		});
	}
}

test('every container case is checked', () => assert.equal(refused, 10));

test('a refusal leaves the module ready for the next file', () => {
	const triangle = new Uint8Array(readFileSync(join(conformance, 'valid/triangle.bread')));
	assert.throws(() => bread.readChunks(triangle.subarray(0, 5)), { reason: 'not-bread' });
	assert.equal(bread.readChunks(triangle).length, 2);
});
