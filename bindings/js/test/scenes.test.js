// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// decodeScene and placeMeshes against the conformance files, and against
// scenes built here for what no conformance file holds: references to
// subtrees. Run `npm run build` first.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { BreadError, load, placeMeshes } from '../src/index.js';

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

/** A node of a conformance JSON, in the shape decodeScene gives. */
function expectedNode(node) {
	const { kind, meta, transform, ...rest } = node;
	return { kind, meta, transform, ...rest };
}

for (const { name, bytes, json } of cases('valid')) {
	test(`valid/${name}: its scene`, () => {
		const scene = bread.decodeScene(bread.readChunks(bytes));
		assert.deepEqual(scene, {
			meta: json.meta,
			meshCount: json.meshes.length,
			meshResources: json.mesh_resources,
			nodes: json.nodes.map(expectedNode),
			subtrees: json.subtrees,
			roots: json.roots,
		});
	});
}

for (const { name, bytes, json } of cases('invalid')) {
	if (json.chunk !== 'SCNE' || !['malformed', 'chunk-count'].includes(json.reason)) continue;
	test(`invalid/${name}: refused as ${json.reason}`, () => {
		const chunks = bread.readChunks(bytes);
		assert.throws(
			() => bread.decodeScene(chunks),
			(err) => err instanceof BreadError && err.reason === json.reason && err.chunk === 'SCNE',
		);
	});
}

test('a file with no scene chunk is refused', () => {
	const chunks = bread.readChunks(readFileSync(join(conformance, 'valid/triangle.bread')));
	assert.throws(
		() => bread.decodeScene(chunks.filter((chunk) => chunk.tag !== 'SCNE')),
		(err) => err instanceof BreadError && err.reason === 'chunk-count' && err.chunk === 'SCNE',
	);
});

/** p through a node transform of the JSON or of decodeScene: M p + t. */
function apply(transform, p) {
	if (transform === null) return p;
	return transform.linear.map((row, i) => row[0] * p[0] + row[1] * p[1] + row[2] * p[2] + transform.translation[i]);
}

/** p through a column-major 4 × 4 matrix. */
function applyMatrix(m, p) {
	return [0, 1, 2].map((i) => m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i]);
}

function assertClose(actual, expected, what) {
	for (let i = 0; i < 3; i++) {
		const scale = Math.max(1, Math.abs(expected[i]));
		assert.ok(Math.abs(actual[i] - expected[i]) <= 1e-12 * scale, `${what}: ${actual} against ${expected}`);
	}
}

const POINTS = [
	[0, 0, 0],
	[1, 0, 0],
	[0, 1, 0],
	[0, 0, 1],
	[3.5, -2.25, 7],
];

/**
 * Every leaf the scene shows, with the nodes from its root down to it, by a
 * plain recursive walk of SPEC.md 5.4.
 */
function walk(scene) {
	const out = [];
	const visit = (index, path) => {
		const node = scene.nodes[index];
		const here = [...path, index];
		if (node.kind === 'group') for (const child of node.children) visit(child, here);
		else if (node.kind === 'reference') visit(scene.subtrees[node.subtree], here);
		else out.push({ leaf: index, path: here });
	};
	for (const root of scene.roots) visit(root, []);
	return out;
}

/** Checks placeMeshes against the plain walk: same leaves, meshes and places. */
function checkPlacements(scene) {
	const placed = placeMeshes(scene);
	const walked = walk(scene);
	assert.deepEqual(
		placed.map((p) => p.leaf),
		walked.map((w) => w.leaf),
	);
	for (const [i, { leaf, mesh, matrix }] of placed.entries()) {
		const node = scene.nodes[leaf];
		assert.equal(mesh, node.kind === 'leaf' ? node.mesh : scene.meshResources[node.resource]);
		assert.equal(matrix.length, 16);
		assert.deepEqual([matrix[3], matrix[7], matrix[11], matrix[15]], [0, 0, 0, 1]);
		for (const p of POINTS) {
			// Through each node's transform in turn, leaf first.
			const expected = walked[i].path.reduceRight((q, index) => apply(scene.nodes[index].transform, q), p);
			assertClose(applyMatrix(matrix, p), expected, `leaf ${leaf}`);
		}
	}
	return placed;
}

for (const { name, bytes } of cases('valid')) {
	test(`valid/${name}: its meshes placed`, () => {
		checkPlacements(bread.decodeScene(bread.readChunks(bytes)));
	});
}

test('a shared mesh is placed by each leaf that shows it', () => {
	const bytes = readFileSync(join(conformance, 'valid/shared-mesh.bread'));
	const placed = placeMeshes(bread.decodeScene(bread.readChunks(bytes)));
	assert.deepEqual(
		placed.map((p) => [p.leaf, p.mesh]),
		[
			[0, 0],
			[1, 0],
		],
	);
	// The second leaf turns, scales and moves: its matrix is its transform.
	assert.deepEqual(Array.from(placed[1].matrix), [0, 0.5, 0, 0, -0.5, 0, 0, 0, 0, 0, 0.5, 0, -3.25, 0.1, 0.001, 1]);
});

// --- Scenes built here --------------------------------------------------------

/** A scene payload, as SPEC.md 5.1 lays it out. */
class Payload {
	bytes = [];
	u8(v) {
		this.bytes.push(v);
		return this;
	}
	u32(v) {
		this.bytes.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, v >>> 24);
		return this;
	}
	text(s) {
		if (s === null) return this.u8(0);
		const utf8 = new TextEncoder().encode(s);
		this.u8(1).u32(utf8.length);
		this.bytes.push(...utf8);
		return this;
	}
	meta(m) {
		return m === null ? this.u8(0) : this.u8(1).text(m.name).text(m.comment);
	}
	transform(t) {
		if (t === null) return this.u8(0);
		this.u8(1);
		const view = new DataView(new ArrayBuffer(8));
		for (const v of [...t.linear.flat(), ...t.translation]) {
			view.setFloat64(0, v, true);
			this.bytes.push(...new Uint8Array(view.buffer));
		}
		return this;
	}
	list(values) {
		this.u32(values.length);
		for (const v of values) this.u32(v);
		return this;
	}
}

/**
 * The chunks of a file with `scene`, its mesh chunks stand-ins: decodeScene
 * only counts them. Records go in the order of `records`: nodes as they are,
 * and `{ subtree: root }` for a subtree record.
 */
function chunksOf(scene, records) {
	const p = new Payload().meta(scene.meta).u32(scene.meshCount).list(scene.meshResources);
	p.u32(records.length);
	const KIND = { group: 0, leaf: 1, 'shared leaf': 2, reference: 4 };
	for (const record of records) {
		if ('root' in record) {
			p.u8(5).u32(record.root);
			continue;
		}
		p.u8(KIND[record.kind]).meta(record.meta).transform(record.transform);
		if (record.kind === 'group') p.list(record.children);
		else p.u32(record.mesh ?? record.resource ?? record.subtree);
	}
	p.list(scene.roots);
	const mesh = { tag: 'MESH', version: 2, payload: new Uint8Array() };
	return [{ tag: 'SCNE', version: 1, payload: new Uint8Array(p.bytes) }, ...Array(scene.meshCount).fill(mesh)];
}

const turned = (x) => ({
	linear: [
		[0, -1, 0],
		[1, 0, 0],
		[0, 0, 2],
	],
	translation: [x, 0.25, -1.5],
});

test('references copy their subtree, each with its own transform', () => {
	const nodes = [
		{ kind: 'leaf', meta: { name: 'arm', comment: null }, transform: turned(1), mesh: 0 },
		{ kind: 'shared leaf', meta: null, transform: null, resource: 0 },
		{ kind: 'group', meta: { name: 'limb', comment: 'é, ü' }, transform: turned(0.5), children: [0, 1] },
		{ kind: 'reference', meta: { name: 'left', comment: null }, transform: turned(-10), subtree: 0 },
		{ kind: 'reference', meta: { name: 'right', comment: null }, transform: turned(10), subtree: 0 },
		{ kind: 'group', meta: null, transform: null, children: [3, 4] },
		{ kind: 'leaf', meta: { name: 'unplaced', comment: null }, transform: null, mesh: 1 },
	];
	const scene = {
		meta: { name: 'kit', comment: null },
		meshCount: 2,
		meshResources: [1],
		nodes,
		subtrees: [2],
		roots: [5],
	};
	const records = [nodes[0], nodes[1], nodes[2], { root: 2 }, nodes[3], nodes[4], nodes[5], nodes[6]];
	const decoded = bread.decodeScene(chunksOf(scene, records));
	assert.deepEqual(decoded, scene);

	const placed = checkPlacements(decoded);
	// Each copy shows both leaves; the subtree's own place and the unplaced
	// leaf show nothing.
	assert.deepEqual(
		placed.map((p) => [p.leaf, p.mesh]),
		[
			[0, 0],
			[1, 1],
			[0, 0],
			[1, 1],
		],
	);
	assert.notDeepEqual(placed[0].matrix, placed[2].matrix);
});

test('a scene that copies itself out of hand is stopped', () => {
	// Subtree k is a group of two references to subtree k - 1: 2^40 leaves
	// from a few hundred bytes.
	const nodes = [{ kind: 'leaf', meta: null, transform: null, mesh: 0 }];
	const records = [nodes[0], { root: 0 }];
	for (let k = 1; k <= 40; k++) {
		const a = { kind: 'reference', meta: null, transform: turned(k), subtree: k - 1 };
		const b = { kind: 'reference', meta: null, transform: null, subtree: k - 1 };
		const group = { kind: 'group', meta: null, transform: null, children: [nodes.length, nodes.length + 1] };
		nodes.push(a, b, group);
		records.push(a, b, group, { root: nodes.length - 1 });
	}
	const top = { kind: 'reference', meta: null, transform: null, subtree: 40 };
	nodes.push(top);
	records.push(top);
	const scene = { meta: null, meshCount: 1, meshResources: [], nodes, subtrees: [], roots: [nodes.length - 1] };
	const decoded = bread.decodeScene(chunksOf(scene, records));
	assert.equal(decoded.subtrees.length, 41);
	assert.throws(() => placeMeshes(decoded), RangeError);
	assert.throws(() => placeMeshes(decoded, { limit: 100 }), RangeError);

	// The same scene showing subtree 3 instead: 8 leaves. Subtree k walks its
	// group, two references and two copies of subtree k - 1: 1, 5, 13 and 29
	// nodes, and the top reference makes 30.
	top.subtree = 3;
	const small = bread.decodeScene(chunksOf(scene, records));
	assert.equal(checkPlacements(small).length, 8);
	assert.equal(placeMeshes(small, { limit: 30 }).length, 8);
	assert.throws(() => placeMeshes(small, { limit: 29 }), RangeError);
});
