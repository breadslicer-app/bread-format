// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// readStl, writeFile and fromStl, through bread-write.wasm. Run
// `npm run build` first.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { BreadError, load } from '../src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
const conformance = join(here, '../../../conformance');
const dist = (name) => {
	const path = join(here, '../dist', name);
	if (!existsSync(path)) throw new Error(`dist/${name} is missing: run \`npm run build\` first`);
	return readFileSync(path);
};
const writer = await load(dist('bread-write.wasm'));
const reader = await load(dist('bread.wasm'));

/** A binary STL of `triangles`, each three corners of x, y and z. */
function binaryStl(triangles, header = '') {
	const bytes = new Uint8Array(84 + 50 * triangles.length);
	const view = new DataView(bytes.buffer);
	bytes.set(new TextEncoder().encode(header));
	view.setUint32(80, triangles.length, true);
	triangles.forEach((triangle, i) => {
		const at = 84 + 50 * i;
		view.setFloat32(at, 0.25, true);
		triangle.flat().forEach((value, j) => {
			if (typeof value === 'bigint') view.setUint32(at + 12 + 4 * j, Number(value), true);
			else view.setFloat32(at + 12 + 4 * j, value, true);
		});
	});
	return bytes;
}

/** The bits of each corner a mesh gives back, three per triangle. */
function corners(mesh) {
	const bits = new Uint32Array(mesh.vertices.buffer, mesh.vertices.byteOffset, mesh.vertices.length);
	return Array.from(mesh.triangles, (v) => [bits[3 * v], bits[3 * v + 1], bits[3 * v + 2]]);
}

/** The bits of the corners of an STL made by `binaryStl`. */
function stlCorners(bytes) {
	const view = new DataView(bytes.buffer, bytes.byteOffset);
	const out = [];
	for (let i = 0; i < view.getUint32(80, true); i++) {
		for (let c = 0; c < 3; c++) {
			const at = 84 + 50 * i + 12 + 12 * c;
			out.push([0, 1, 2].map((k) => view.getUint32(at + 4 * k, true)));
		}
	}
	return out;
}

// 0x7fc01234 is a NaN with a payload, 0x80000000 is -0.0.
const SQUARE = [
	[
		[0, 0, 0],
		[1, 0, 0],
		[1, 1, 0],
	],
	[
		[0x80000000n, 0, 0],
		[1, 1, 0],
		[0, 1, 0x7fc01234n],
	],
];

test('the reading module does not write', () => {
	assert.equal(reader.canWrite, false);
	assert.equal(writer.canWrite, true);
	assert.throws(() => reader.readStl(binaryStl(SQUARE)), TypeError);
	assert.throws(() => reader.writeFile({ nodes: [], roots: [] }, []), TypeError);
});

test('a binary STL is read corner for corner', () => {
	const stl = binaryStl(SQUARE, 'solid exported part');
	const mesh = writer.readStl(stl);
	assert.deepEqual(corners(mesh), stlCorners(stl));
	assert.equal(mesh.vertices.length / 3, 5, '-0.0 and 0.0 are two vertices');
	assert.deepEqual(mesh.meta, { name: null, comment: 'solid exported part' });
});

test('an ASCII STL is read', () => {
	const text = 'solid cube part\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n';
	const mesh = writer.readStl(new TextEncoder().encode(text));
	assert.deepEqual(Array.from(mesh.triangles), [0, 1, 2]);
	assert.deepEqual(mesh.meta, { name: 'cube part', comment: null });
});

test('what is not an STL is refused', () => {
	const refused = (bytes, reason) =>
		assert.throws(
			() => writer.readStl(bytes),
			(err) => err instanceof BreadError && err.reason === reason && err.chunk === null,
		);
	refused(new TextEncoder().encode('not an stl'), 'not-stl');
	refused(new Uint8Array(), 'not-stl');
	refused(new TextEncoder().encode('solid\nvertex 1 2 3\nendsolid\n'), 'bad-stl');
	refused(binaryStl(SQUARE).subarray(1), 'not-stl');
});

function cases() {
	return readdirSync(join(conformance, 'valid'))
		.filter((name) => name.endsWith('.bread'))
		.map((name) => ({ name, bytes: new Uint8Array(readFileSync(join(conformance, 'valid', name))) }));
}

/** A file's scene and meshes, read with the reading module. */
function readAll(bytes) {
	const chunks = reader.readChunks(bytes);
	return {
		scene: reader.decodeScene(chunks),
		meshes: chunks.filter((c) => c.tag === 'MESH').map((c) => reader.decodeMesh(c)),
	};
}

/** Bit for bit, NaN payloads and -0.0 included. */
function sameMeshes(a, b) {
	assert.equal(a.length, b.length);
	for (let i = 0; i < a.length; i++) {
		assert.deepEqual(a[i].meta, b[i].meta);
		assert.deepEqual(new Uint8Array(a[i].vertices.buffer), new Uint8Array(b[i].vertices.buffer));
		assert.deepEqual(a[i].triangles, b[i].triangles);
	}
}

for (const { name, bytes } of cases()) {
	// The slicer chunks are not written by writeFile: their files' scenes and
	// meshes still are.
	test(`valid/${name}: written again, it reads back the same`, () => {
		const { scene, meshes } = readAll(bytes);
		const written = writer.writeFile(scene, meshes);
		const back = readAll(written);
		assert.deepEqual(back.scene, scene);
		sameMeshes(back.meshes, meshes);
	});
}

test('a scene with references and subtrees is written', () => {
	const turned = (x) => ({
		linear: [
			[0, -1, 0],
			[1, 0, 0],
			[0, 0, 2],
		],
		translation: [x, 0.25, -1.5],
	});
	const meshes = [writer.readStl(binaryStl(SQUARE)), writer.readStl(binaryStl(SQUARE.slice(0, 1)))];
	const scene = {
		meta: { name: 'kit', comment: '' },
		meshCount: 2,
		meshResources: [1],
		nodes: [
			{ kind: 'leaf', meta: { name: 'arm', comment: null }, transform: turned(1), mesh: 0 },
			{ kind: 'shared leaf', meta: null, transform: null, resource: 0 },
			{ kind: 'group', meta: { name: 'limb', comment: 'é, ü' }, transform: turned(0.5), children: [0, 1] },
			{ kind: 'reference', meta: { name: 'left', comment: null }, transform: turned(-10), subtree: 0 },
			{ kind: 'reference', meta: { name: 'right', comment: null }, transform: turned(10), subtree: 0 },
			{ kind: 'group', meta: null, transform: null, children: [3, 4] },
		],
		subtrees: [2],
		roots: [5],
	};
	const back = readAll(writer.writeFile(scene, meshes));
	assert.deepEqual(back.scene, scene);
	sameMeshes(back.meshes, meshes);
});

test('a scene a reader would refuse is not written', () => {
	const mesh = writer.readStl(binaryStl(SQUARE));
	const leaf = { kind: 'leaf', meta: null, transform: null, mesh: 0 };
	const refused = (scene, meshes, reason, chunk) =>
		assert.throws(
			() => writer.writeFile(scene, meshes),
			(err) => err instanceof BreadError && err.reason === reason && err.chunk === chunk,
		);
	// A leaf of a mesh that is not there.
	refused({ nodes: [{ ...leaf, mesh: 1 }], roots: [0] }, [mesh], 'malformed', 'SCNE');
	// A node with two places.
	refused({ nodes: [leaf], roots: [0, 0] }, [mesh], 'malformed', 'SCNE');
	// A triangle past the vertices.
	const broken = { ...mesh, triangles: Uint32Array.of(0, 1, 9) };
	refused({ nodes: [leaf], roots: [0] }, [broken], 'no-such-vertex', 'MESH');
	// Vertices that are not whole.
	refused({ nodes: [leaf], roots: [0] }, [{ ...mesh, vertices: new Float32Array(4) }], 'malformed', 'MESH');
	// The module is left ready for the next file.
	assert.equal(readAll(writer.writeFile({ nodes: [leaf], roots: [0] }, [mesh])).meshes.length, 1);
});

test('STL files become one .bread file, one leaf each', () => {
	const a = binaryStl(SQUARE);
	const b = new TextEncoder().encode(
		'solid b\nfacet normal 0 0 1\nouter loop\nvertex 5 0 0\nvertex 6 0 0\nvertex 5 1 0\nendloop\nendfacet\nendsolid b\n',
	);
	const { file, meshes } = writer.fromStl(
		[
			{ name: 'arm.stl', bytes: a },
			{ name: 'base.stl', bytes: b },
		],
		{ name: 'kit' },
	);
	const back = readAll(file);
	assert.deepEqual(back.scene.meta, { name: 'kit', comment: null });
	assert.deepEqual(
		back.scene.nodes.map((n) => [n.kind, n.meta.name, n.mesh, n.transform]),
		[
			['leaf', 'arm.stl', 0, null],
			['leaf', 'base.stl', 1, null],
		],
	);
	assert.deepEqual(back.scene.roots, [0, 1]);
	sameMeshes(back.meshes, meshes);
	assert.deepEqual(corners(back.meshes[0]), stlCorners(a));
	assert.equal(back.meshes[1].meta.name, 'b');
});
