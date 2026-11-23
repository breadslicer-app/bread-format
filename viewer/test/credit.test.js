// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Roles and metadata in the viewer (src/core.js), as built in dist/: what
// each part shows, and who made the file. Run `npm run build` in bindings/js
// and then here first.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, '../dist');
if (!existsSync(join(dist, 'core.js'))) throw new Error('viewer/dist is missing: run `npm run build` first');
const { load } = await import(new URL('../dist/bread.js', import.meta.url));
const core = await import(new URL('../dist/core.js', import.meta.url));

const reader = await load(readFileSync(join(dist, 'bread.wasm')));
const conformance = join(here, '../../conformance');
const file = (name) => new Uint8Array(readFileSync(join(conformance, name)));

test('supports and rafts are known by their role', () => {
	const opened = core.openBread(reader, file('valid/roles.bread'));
	assert.deepEqual(opened.roles, [
		{ leaf: 2, role: 1 },
		{ leaf: 3, role: 2 },
	]);
	// The leaf named by the supports entry has neither a role nor a
	// preview: its mesh is empty, so it draws nothing either way.
	assert.deepEqual(opened.kinds, ['model', 'model', 'supports', 'raft']);
});

test('a leaf with a support preview shows supports', () => {
	const opened = core.openBread(reader, file('valid/support-preview.bread'));
	const previewed = new Set(opened.previews.map((p) => p.leaf));
	assert.deepEqual(
		opened.kinds,
		opened.placements.map((p) => (previewed.has(p.leaf) ? 'supports' : 'model')),
	);
	assert.ok(opened.kinds.includes('supports'));
});

test('a file with no roles is all models', () => {
	const opened = core.openBread(reader, file('valid/two-parts.bread'));
	assert.deepEqual(opened.kinds, ['model', 'model']);
	assert.deepEqual(opened.metadata, []);
	assert.deepEqual(core.creditOf(opened.metadata), { link: null });
});

test('the credit comes from the metadata about the whole file', () => {
	const opened = core.openBread(reader, file('valid/metadata.bread'));
	assert.deepEqual(core.creditOf(opened.metadata), {
		title: 'Box with a lid',
		designer: 'A. Designer',
		license: 'CC-BY-4.0',
		source: 'https://example.com/box',
		copyright: '\u00a9 2026 A. Designer',
		description: 'A test box, in two parts.',
		link: 'https://example.com/box',
	});
});

test('entries about a node, and values that are not text, are not the credit', () => {
	const credit = core.creditOf([
		{ node: 0, key: 'designer', text: 'Someone Else' },
		{ node: null, key: 'designer', text: 'A. Designer' },
		{ node: null, key: 'example.title', text: null },
	]);
	assert.deepEqual(credit, { designer: 'A. Designer', link: null });
});

test('a source is a link only when it is an http or https address', () => {
	assert.equal(core.webLink('https://example.com/box'), 'https://example.com/box');
	assert.equal(core.webLink('  http://example.com  '), 'http://example.com/');
	for (const text of [
		'javascript:alert(1)',
		'JavaScript:alert(1)',
		'data:text/html,<b>hi</b>',
		'file:///C:/secret',
		'example.com/box',
		'a printed catalogue, page 4',
	]) {
		assert.equal(core.webLink(text), null, text);
	}
	const credit = core.creditOf([{ node: null, key: 'source', text: 'javascript:alert(1)' }]);
	assert.deepEqual(credit, { source: 'javascript:alert(1)', link: null });
});
