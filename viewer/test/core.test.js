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

test('a support preview is drawn where its leaf is', () => {
	const opened = core.openBread(reader, file('valid/support-preview.bread'));
	assert.equal(opened.previews.length, 1);
	const support = opened.scene.nodes.findIndex((n) => n.meta?.name === 'supports');
	assert.equal(opened.previews[0].leaf, support);
	// The model has no preview; the support leaf has the one there is.
	assert.deepEqual(core.previewOf(opened), [-1, 0]);
	assert.ok(opened.previews[0].mesh.triangles.length > 0);
	assert.deepEqual(core.openBread(reader, file('valid/triangle.bread')).previews, []);
});

test('a damaged file is refused for its reason', () => {
	assert.throws(() => core.openBread(reader, file('invalid/crc-mismatch.bread')), { reason: 'crc' });
	assert.throws(() => core.openBread(reader, file('invalid/no-such-mesh.bread')), { reason: 'malformed' });
});

const writer = await load(readFileSync(join(dist, 'bread-write.wasm')));

/** A binary STL of `triangles`, each three corners of x, y and z. */
function binaryStl(triangles) {
	const bytes = new Uint8Array(84 + 50 * triangles.length);
	const view = new DataView(bytes.buffer);
	view.setUint32(80, triangles.length, true);
	triangles.forEach((triangle, i) => {
		triangle.flat().forEach((value, j) => view.setFloat32(84 + 50 * i + 12 + 4 * j, value, true));
	});
	return bytes;
}

const TETRA = [
	[
		[0, 0, 0],
		[10, 0, 0],
		[0, 10, 0],
	],
	[
		[0, 0, 0],
		[0, 10, 0],
		[0, 0, -0],
	],
	[
		[10, 0, 0],
		[0, 10, 0],
		[0.1, 0.2, 7.25],
	],
];

test('STL files become one checked .bread file', () => {
	const files = [
		{ name: 'a.stl', bytes: binaryStl(TETRA) },
		{
			name: 'b.stl',
			bytes: new TextEncoder().encode(
				'solid b\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n',
			),
		},
	];
	const result = core.convertStl(writer, files, { name: 'kit' });
	assert.equal(result.verified, true);
	assert.equal(result.stlBytes, files[0].bytes.length + files[1].bytes.length);
	assert.deepEqual(
		result.parts.map((p) => [p.name, p.triangles, p.vertices]),
		[
			['a.stl', 3, 5],
			['b.stl', 1, 3],
		],
	);
	// What the viewer then opens is what was written.
	const opened = core.openBread(reader, result.file);
	assert.deepEqual(
		opened.scene.nodes.map((n) => n.meta.name),
		['a.stl', 'b.stl'],
	);
	assert.equal(core.sameAsBinaryStl(opened.meshes[0], files[0].bytes), true);
	assert.equal(core.sameAsBinaryStl(opened.meshes[1], files[1].bytes), null, 'ASCII is checked another way');
});

test('a mesh that does not give back its STL fails the check', () => {
	const stl = binaryStl(TETRA);
	const mesh = writer.readStl(stl);
	assert.equal(core.sameAsBinaryStl(mesh, stl), true);
	const moved = { ...mesh, vertices: mesh.vertices.slice() };
	moved.vertices[2] = 0.5;
	assert.equal(core.sameAsBinaryStl(moved, stl), false);
	assert.equal(core.sameAsBinaryStl({ ...mesh, triangles: mesh.triangles.subarray(3) }, stl), false);
});

test('an STL that cannot be read is refused for its reason', () => {
	assert.throws(() => core.convertStl(writer, [{ name: 'x.stl', bytes: new Uint8Array(10) }]), {
		reason: 'not-stl',
	});
});

test('placed parts are written as one STL, in world positions', () => {
	const opened = core.openBread(reader, file('valid/two-parts.bread'));
	const parts = opened.placements.map((p) => ({ mesh: opened.meshes[p.mesh], matrix: p.matrix }));
	const stl = core.toBinaryStl(parts);
	const total = parts.reduce((sum, p) => sum + p.mesh.triangles.length / 3, 0);
	assert.equal(stl.length, core.binaryStlSize(total));
	assert.equal(new TextDecoder().decode(stl.subarray(0, 5)) === 'solid', false, 'a binary header must not say solid');
	// Read back, each corner is where its leaf's transform puts it.
	const back = writer.readStl(stl);
	let corner = 0;
	for (const { mesh, matrix: m } of parts) {
		for (const v of mesh.triangles) {
			const [x, y, z] = mesh.vertices.subarray(3 * v, 3 * v + 3);
			const want = [
				Math.fround(m[0] * x + m[4] * y + m[8] * z + m[12]),
				Math.fround(m[1] * x + m[5] * y + m[9] * z + m[13]),
				Math.fround(m[2] * x + m[6] * y + m[10] * z + m[14]),
			];
			const got = back.vertices.subarray(3 * back.triangles[corner], 3 * back.triangles[corner] + 3);
			assert.deepEqual(Array.from(got), want);
			corner++;
		}
	}
});

test('a mirroring transform keeps the faces facing out', () => {
	const mesh = { vertices: Float32Array.of(0, 0, 0, 1, 0, 0, 0, 1, 0), triangles: Uint32Array.of(0, 1, 2) };
	const mirror = Float64Array.of(-1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1);
	const back = writer.readStl(core.toBinaryStl([{ mesh, matrix: mirror }]));
	const corners = Array.from(back.triangles, (v) => Array.from(back.vertices.subarray(3 * v, 3 * v + 3)));
	// (0,0,0), (0,1,0), (-1,0,0): the mirrored triangle, turned back to counterclockwise from +Z.
	assert.deepEqual(corners, [
		[0, 0, 0],
		[0, 1, 0],
		[-1, 0, 0],
	]);
});

test('placed sizes are in millimetres, in world positions', () => {
	const cube = { vertices: Float32Array.of(0, 0, 0, 1, 2, 3, NaN, 0, 0), triangles: Uint32Array.of(0, 1, 2) };
	const identity = Float64Array.of(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1);
	assert.deepEqual(core.placedSize(cube, identity), [1, 2, 3]);
	// Turned a quarter around Z, and doubled.
	const turned = Float64Array.of(0, 2, 0, 0, -2, 0, 0, 0, 0, 0, 2, 0, 5, 5, 5, 1);
	assert.deepEqual(core.placedSize(cube, turned), [4, 2, 6]);
});

test('side by side reads an STL straight from its bytes', () => {
	const stl = binaryStl(TETRA);
	assert.equal(core.binaryStlTriangles(stl), 3);
	assert.equal(core.binaryStlTriangles(stl.subarray(0, stl.length - 1)), null, 'a size that does not fit its count');
	const corners = core.stlCorners(stl);
	assert.deepEqual(Array.from(corners), TETRA.flat(2).map(Math.fround));
	// -0 keeps its sign bit: every number has the file's bits.
	assert.equal(new Uint32Array(corners.buffer)[17], 0x80000000);
	const triangle = core.stlTriangle(stl, 2);
	assert.deepEqual(triangle.corners[2].values, [0.1, 0.2, 7.25].map(Math.fround));
	assert.equal(core.bitsText(triangle.corners[2].bits[2]), '0x40E80000');
});

test('side by side finds the same triangle, corner by corner, in the .bread file', () => {
	const stl = binaryStl(TETRA);
	const result = core.convertStl(writer, [{ name: 'a.stl', bytes: stl }]);
	const mesh = core.openBread(reader, result.file).meshes[0];
	for (let i = 0; i < 3; i++) {
		const left = core.stlTriangle(stl, i).corners.map((c) => c.bits);
		const right = core.meshTriangle(mesh, i).corners.map((c) => c.bits);
		assert.deepEqual(right, left);
	}
	// Two corners of the STL that are one vertex in the .bread file.
	assert.equal(core.meshTriangle(mesh, 0).corners[0].vertex, core.meshTriangle(mesh, 1).corners[0].vertex);
});

test('the numbers are written as the shortest decimal of their f32', () => {
	assert.equal(core.f32Text(Math.fround(0.1)), '0.1');
	assert.equal(core.f32Text(Math.fround(7.25)), '7.25');
	assert.equal(core.f32Text(Math.fround(1 / 3)), '0.33333334');
	assert.equal(core.f32Text(-0), '-0');
	assert.equal(core.f32Text(NaN), 'NaN');
	for (const x of [Math.fround(123.456), Math.fround(-1e-7), Math.fround(3.4e38)]) {
		assert.equal(Math.fround(Number(core.f32Text(x))), x);
	}
});

test('the grid finds the triangles near a point', () => {
	// A strip of 1,000 triangles along X, one millimetre each.
	const strip = [];
	for (let i = 0; i < 1000; i++) {
		strip.push([
			[i, 0, 0],
			[i + 1, 0, 0],
			[i, 1, 0],
		]);
	}
	const grid = core.triangleGrid(binaryStl(strip));
	const near = core.nearTriangles(grid, [500.3, 0.3, 0], 2.5).sort((a, b) => a - b);
	// Centres at x = i + 1/3: within 2.5 of 500.3 are 498 to 502.
	assert.deepEqual(near, [498, 499, 500, 501, 502]);
	assert.equal(core.nearTriangles(grid, [500, 0, 0], 400, 100), null, 'more than the limit');
	assert.deepEqual(core.nearTriangles(grid, [5000, 0, 0], 2), []);
});

test('a ray meets a triangle from either side', () => {
	const [a, b, c] = [
		[0, 0, 0],
		[10, 0, 0],
		[0, 10, 0],
	];
	assert.equal(core.rayTriangle([2, 2, 5], [0, 0, -1], a, b, c), 5);
	assert.equal(core.rayTriangle([2, 2, -5], [0, 0, 1], a, b, c), 5);
	assert.equal(core.rayTriangle([20, 20, 5], [0, 0, -1], a, b, c), null);
	assert.equal(core.rayTriangle([2, 2, 5], [0, 0, 1], a, b, c), null, 'behind the eye');
});
