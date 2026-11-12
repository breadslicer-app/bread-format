// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// What the viewer does with files, apart from the page: open a .bread file,
// turn STL files into one and check it, and write the scene back out as STL.
// The worker runs it off the page's thread; the tests run it in Node.

import { placeMeshes } from './bread.js';

/**
 * A `.bread` file, read: its scene, its meshes, and every leaf shown with the
 * mesh it shows and where.
 *
 * @param {import('./bread.js').Bread} bread a loaded module
 * @param {Uint8Array} bytes
 */
export function openBread(bread, bytes) {
	const chunks = bread.readChunks(bytes);
	const scene = bread.decodeScene(chunks);
	const meshes = chunks.filter((chunk) => chunk.tag === 'MESH').map((chunk) => bread.decodeMesh(chunk));
	const placements = placeMeshes(scene);
	return { scene, meshes, placements, chunks: chunks.map((chunk) => chunk.tag) };
}

/** The bits of a float array, to compare without NaN or -0.0 getting in the way. */
function bitsOf(floats) {
	return new Uint32Array(floats.buffer, floats.byteOffset, floats.length);
}

function sameArrays(a, b) {
	if (a.length !== b.length) return false;
	for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
	return true;
}

/**
 * Whether `mesh` gives back the corners of a binary STL, bit for bit and in
 * order; `null` when the STL is not binary, which is checked against what
 * was read from it instead.
 */
export function sameAsBinaryStl(mesh, stl) {
	if (stl.length < 84) return null;
	const view = new DataView(stl.buffer, stl.byteOffset, stl.byteLength);
	const count = view.getUint32(80, true);
	if (stl.length !== 84 + 50 * count) return null;
	if (mesh.triangles.length !== 3 * count) return false;
	const bits = bitsOf(mesh.vertices);
	for (let i = 0; i < mesh.triangles.length; i++) {
		const at = 84 + 50 * Math.floor(i / 3) + 12 + 12 * (i % 3);
		const v = 3 * mesh.triangles[i];
		for (let k = 0; k < 3; k++) {
			if (bits[v + k] !== view.getUint32(at + 4 * k, true)) return false;
		}
	}
	return true;
}

/**
 * STL files as one `.bread` file, each a part named after its file, and the
 * file checked before it is handed over: read back, every mesh is compared
 * with what was read from its STL, and every corner of a binary STL with the
 * STL's own bytes, all bit for bit.
 *
 * @param {import('./bread.js').Bread} writer a module that writes
 * @param {{ name: string, bytes: Uint8Array }[]} files
 * @returns {{ file: Uint8Array, verified: boolean, stlBytes: number,
 *   parts: { name: string, triangles: number, vertices: number }[] }}
 */
export function convertStl(writer, files, options = {}) {
	const { file, meshes } = writer.fromStl(files, options);
	const back = openBread(writer, file).meshes;
	let verified = back.length === meshes.length;
	for (let i = 0; verified && i < meshes.length; i++) {
		verified =
			sameArrays(bitsOf(back[i].vertices), bitsOf(meshes[i].vertices)) &&
			sameArrays(back[i].triangles, meshes[i].triangles) &&
			sameAsBinaryStl(back[i], files[i].bytes) !== false;
	}
	return {
		file,
		verified,
		stlBytes: files.reduce((sum, f) => sum + f.bytes.length, 0),
		parts: files.map((f, i) => ({
			name: f.name,
			triangles: meshes[i].triangles.length / 3,
			vertices: meshes[i].vertices.length / 3,
		})),
	};
}

/** The size of a binary STL of `triangles` triangles. */
export function binaryStlSize(triangles) {
	return 84 + 50 * triangles;
}

/**
 * A binary STL of placed meshes, in world positions: each vertex moved by
 * its leaf's matrix in f64, then rounded to f32. Facet normals are written as
 * zero, which programs read as "work it out".
 *
 * @param {{ mesh: { vertices: Float32Array, triangles: Uint32Array }, matrix: Float64Array }[]} parts
 * @param {string} [header] up to 80 bytes of ASCII
 */
export function toBinaryStl(parts, header = 'binary STL written from a .bread file') {
	const triangles = parts.reduce((sum, p) => sum + p.mesh.triangles.length / 3, 0);
	const out = new Uint8Array(binaryStlSize(triangles));
	const view = new DataView(out.buffer);
	const head = new TextEncoder().encode(header.replace(/^solid/i, 'STL')).subarray(0, 80);
	out.set(head);
	view.setUint32(80, triangles, true);
	let at = 84;
	for (const { mesh, matrix: m } of parts) {
		const { vertices, triangles: t } = mesh;
		// Every vertex placed once.
		const placed = new Float32Array(vertices.length);
		for (let i = 0; i < vertices.length; i += 3) {
			const x = vertices[i];
			const y = vertices[i + 1];
			const z = vertices[i + 2];
			placed[i] = m[0] * x + m[4] * y + m[8] * z + m[12];
			placed[i + 1] = m[1] * x + m[5] * y + m[9] * z + m[13];
			placed[i + 2] = m[2] * x + m[6] * y + m[10] * z + m[14];
		}
		// A transform that mirrors turns the faces inside out: keep them
		// facing out by swapping two corners.
		const det =
			m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2]);
		const order = det < 0 ? [0, 2, 1] : [0, 1, 2];
		for (let i = 0; i < t.length; i += 3) {
			at += 12;
			for (const c of order) {
				const v = 3 * t[i + c];
				view.setFloat32(at, placed[v], true);
				view.setFloat32(at + 4, placed[v + 1], true);
				view.setFloat32(at + 8, placed[v + 2], true);
				at += 12;
			}
			at += 2;
		}
	}
	return out;
}

/** The size of a placed mesh's box, in millimetres: x, y and z. */
export function placedSize(mesh, matrix) {
	const min = [Infinity, Infinity, Infinity];
	const max = [-Infinity, -Infinity, -Infinity];
	const v = mesh.vertices;
	const m = matrix;
	for (let i = 0; i < v.length; i += 3) {
		const p = [
			m[0] * v[i] + m[4] * v[i + 1] + m[8] * v[i + 2] + m[12],
			m[1] * v[i] + m[5] * v[i + 1] + m[9] * v[i + 2] + m[13],
			m[2] * v[i] + m[6] * v[i + 1] + m[10] * v[i + 2] + m[14],
		];
		if (!p.every(Number.isFinite)) continue;
		for (let k = 0; k < 3; k++) {
			if (p[k] < min[k]) min[k] = p[k];
			if (p[k] > max[k]) max[k] = p[k];
		}
	}
	return min[0] > max[0] ? [0, 0, 0] : [0, 1, 2].map((k) => max[k] - min[k]);
}
