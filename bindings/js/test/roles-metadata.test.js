// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// decodeRoles and decodeMetadata against the conformance files. Run
// `npm run build` first.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { BreadError, ROLE_RAFT, ROLE_SUPPORTS, load } from '../src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
const conformance = join(here, '../../../conformance');
const wasm = join(here, '../dist/bread.wasm');
if (!existsSync(wasm)) throw new Error('dist/bread.wasm is missing: run `npm run build` first');
const bread = await load(readFileSync(wasm));

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

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

/** A metadata entry as the conformance JSON writes it. */
function asJson(entry) {
	let value = entry.uuid ?? entry.text;
	if (value === null) value = { size: entry.value.length, sha256: sha256(entry.value) };
	return { subject: entry.node, key: entry.key, value };
}

for (const { name, bytes, json } of cases('valid')) {
	test(`valid/${name}: its roles and metadata`, () => {
		const chunks = bread.readChunks(bytes);
		assert.deepEqual(bread.decodeRoles(chunks), json.roles);
		assert.deepEqual(bread.decodeMetadata(chunks).map(asJson), json.metadata);
	});
}

for (const { name, bytes, json } of cases('invalid')) {
	if (!['ROLE', 'META'].includes(json.chunk)) continue;
	test(`invalid/${name}: refused for ${json.reason}`, () => {
		const chunks = bread.readChunks(bytes);
		const decode = json.chunk === 'ROLE' ? () => bread.decodeRoles(chunks) : () => bread.decodeMetadata(chunks);
		assert.throws(decode, (err) => err instanceof BreadError && err.reason === json.reason && err.chunk === json.chunk);
	});
}

test('the role numbers', () => {
	assert.equal(ROLE_SUPPORTS, 1);
	assert.equal(ROLE_RAFT, 2);
});

test('metadata gives text and ids by their key', () => {
	const { bytes } = cases('valid').find((c) => c.name === 'metadata');
	const entries = bread.decodeMetadata(bread.readChunks(bytes));
	const title = entries.find((e) => e.node === null && e.key === 'title');
	assert.equal(title.text, 'Box with a lid');
	assert.equal(title.uuid, null);
	const id = entries.find((e) => e.node === 0 && e.key === 'uuid');
	assert.match(id.uuid, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
	assert.equal(id.value.length, 16);
	const own = entries.find((e) => e.key === 'example.note');
	assert.deepEqual([own.text, own.uuid, Array.from(own.value)], [null, null, [0, 1, 2, 255]]);
});
