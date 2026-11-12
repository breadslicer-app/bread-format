// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// The viewer's file work (src/core.js), as built in dist/. Run `npm run build`
// in bindings/js and then here first.

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

test('a .bread file opens with its placements', () => {
	const opened = core.openBread(reader, file('valid/two-parts.bread'));
	assert.equal(opened.meshes.length, 2);
	assert.equal(opened.placements.length, 2);
	assert.deepEqual(opened.chunks, ['SCNE', 'MESH', 'MESH']);
});

test('a shared mesh is placed twice', () => {
	const opened = core.openBread(reader, file('valid/shared-mesh.bread'));
	assert.deepEqual(
		opened.placements.map((p) => p.mesh),
		[0, 0],
	);
});

test('a damaged file is refused for its reason', () => {
	assert.throws(() => core.openBread(reader, file('invalid/crc-mismatch.bread')), { reason: 'crc' });
	assert.throws(() => core.openBread(reader, file('invalid/no-such-mesh.bread')), { reason: 'malformed' });
});

test('placed sizes are in millimetres, in world positions', () => {
	const cube = { vertices: Float32Array.of(0, 0, 0, 1, 2, 3, NaN, 0, 0), triangles: Uint32Array.of(0, 1, 2) };
	const identity = Float64Array.of(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1);
	assert.deepEqual(core.placedSize(cube, identity), [1, 2, 3]);
	// Turned a quarter around Z, and doubled.
	const turned = Float64Array.of(0, 2, 0, 0, -2, 0, 0, 0, 0, 0, 2, 0, 5, 5, 5, 1);
	assert.deepEqual(core.placedSize(cube, turned), [4, 2, 6]);
});
