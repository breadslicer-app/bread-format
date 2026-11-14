// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// What the viewer does with files, apart from the page: open a .bread file,
// turn STL files into one and check it, and write the scene back out as STL.
// The worker runs it off the page's thread; the tests run it in Node.

import { placeMeshes } from './bread.js';

/**
 * A `.bread` file, read: its scene, its meshes, every leaf shown with the
 * mesh it shows and where, and the support previews, by leaf.
 *
 * @param {import('./bread.js').Bread} bread a loaded module
 * @param {Uint8Array} bytes
 */
export function openBread(bread, bytes) {
	const chunks = bread.readChunks(bytes);
	const scene = bread.decodeScene(chunks);
	const meshes = chunks.filter((chunk) => chunk.tag === 'MESH').map((chunk) => bread.decodeMesh(chunk));
	const placements = placeMeshes(scene);
	const previews = bread.decodePreviews(chunks, meshes);
	return { scene, meshes, placements, previews, chunks: chunks.map((chunk) => chunk.tag) };
}

/** The support preview of each placement's leaf, as an index into `previews`, or -1. */
export function previewOf(file) {
	const byLeaf = new Map(file.previews.map((p, i) => [p.leaf, i]));
	return file.placements.map((p) => byLeaf.get(p.leaf) ?? -1);
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

// --- The side-by-side comparison ------------------------------------------------
//
// The STL side is read here straight from the file's bytes, without the
// format's code, so what it shows does not depend on what it is compared with.

/** The triangles of a binary STL, or null when `stl` is not one: its size is not what its count says. */
export function binaryStlTriangles(stl) {
	if (stl.length < 84) return null;
	const count = new DataView(stl.buffer, stl.byteOffset, stl.byteLength).getUint32(80, true);
	return stl.length === 84 + 50 * count ? count : null;
}

/** Where corner `c` (0 to 2) of triangle `i` starts in a binary STL. */
const cornerAt = (i, c) => 84 + 50 * i + 12 + 12 * c;

/**
 * A binary STL's corners as one array, three per triangle, nine numbers a
 * triangle: its bytes copied as they are, so every number has the file's bits.
 */
export function stlCorners(stl) {
	const count = binaryStlTriangles(stl);
	const out = new Float32Array(9 * count);
	const bytes = new Uint8Array(out.buffer);
	for (let i = 0; i < count; i++) bytes.set(stl.subarray(cornerAt(i, 0), cornerAt(i, 3)), 36 * i);
	return out;
}

/**
 * Triangle `i` of a binary STL as the file has it: its three corners, each as
 * numbers and as the 32 bits of each `f32`, and its facet normal.
 */
export function stlTriangle(stl, i) {
	const view = new DataView(stl.buffer, stl.byteOffset, stl.byteLength);
	const at = 84 + 50 * i;
	const read = (from) => ({
		values: [0, 1, 2].map((k) => view.getFloat32(from + 4 * k, true)),
		bits: [0, 1, 2].map((k) => view.getUint32(from + 4 * k, true)),
	});
	return { normal: read(at), corners: [0, 1, 2].map((c) => read(cornerAt(i, c))) };
}

/** Triangle `i` of a mesh read from a `.bread` file: each corner's vertex, and its numbers and bits. */
export function meshTriangle(mesh, i) {
	const bits = new Uint32Array(mesh.vertices.buffer, mesh.vertices.byteOffset, mesh.vertices.length);
	return {
		corners: [0, 1, 2].map((c) => {
			const vertex = mesh.triangles[3 * i + c];
			return {
				vertex,
				values: [0, 1, 2].map((k) => mesh.vertices[3 * vertex + k]),
				bits: [0, 1, 2].map((k) => bits[3 * vertex + k]),
			};
		}),
	};
}

/** The 32 bits of an `f32`, as the page shows them: `0x41A3C2F0`. */
export function bitsText(bits) {
	return `0x${(bits >>> 0).toString(16).toUpperCase().padStart(8, '0')}`;
}

/** An `f32` as the shortest decimal that reads back as the same number. */
export function f32Text(x) {
	if (Object.is(x, -0)) return '-0';
	if (!Number.isFinite(x)) return String(x);
	for (let digits = 1; digits <= 9; digits++) {
		const text = x.toPrecision(digits);
		if (Math.fround(Number(text)) === x) return String(Number(text));
	}
	return String(x);
}

const centroid = (view, i, out) => {
	for (let k = 0; k < 3; k++) {
		out[k] =
			(view.getFloat32(cornerAt(i, 0) + 4 * k, true) +
				view.getFloat32(cornerAt(i, 1) + 4 * k, true) +
				view.getFloat32(cornerAt(i, 2) + 4 * k, true)) /
			3;
	}
	return out;
};

const hashCell = (x, y, z, mask) => (Math.imul(x, 73856093) ^ Math.imul(y, 19349663) ^ Math.imul(z, 83492791)) & mask;

/**
 * A grid over a binary STL's triangles, by their centres, to find those near a
 * point without going through all of them. Its cells are about twice a
 * triangle's size, hashed into a table, so a thin or hollow model costs no
 * more than a solid one.
 */
export function triangleGrid(stl) {
	const count = binaryStlTriangles(stl);
	const view = new DataView(stl.buffer, stl.byteOffset, stl.byteLength);
	// A triangle's size: the mean edge of up to 4,096 of them, spread over the mesh.
	let edges = 0;
	let measured = 0;
	const step = Math.max(1, Math.floor(count / 4096));
	for (let i = 0; i < count; i += step) {
		for (let c = 0; c < 3; c++) {
			let d = 0;
			for (let k = 0; k < 3; k++) {
				const a = view.getFloat32(cornerAt(i, c) + 4 * k, true);
				const b = view.getFloat32(cornerAt(i, (c + 1) % 3) + 4 * k, true);
				d += (a - b) ** 2;
			}
			if (Number.isFinite(d)) {
				edges += Math.sqrt(d);
				measured++;
			}
		}
	}
	const cell = edges > 0 ? (2 * edges) / measured : 1;
	let size = 1;
	while (size < count / 2) size *= 2;
	const mask = size - 1;
	const bucketOf = new Uint32Array(count);
	const start = new Uint32Array(size + 1);
	const p = [0, 0, 0];
	for (let i = 0; i < count; i++) {
		centroid(view, i, p);
		const b = hashCell(Math.floor(p[0] / cell), Math.floor(p[1] / cell), Math.floor(p[2] / cell), mask);
		bucketOf[i] = b;
		start[b + 1]++;
	}
	for (let b = 0; b < size; b++) start[b + 1] += start[b];
	const items = new Uint32Array(count);
	const next = start.slice(0, size);
	for (let i = 0; i < count; i++) items[next[bucketOf[i]]++] = i;
	return { stl, view, cell, mask, start, items };
}

/**
 * The triangles whose centre is within `radius` of `point`, or null when there
 * would be more than `limit` (or too many cells to look through): the loupe
 * then asks to come closer.
 */
export function nearTriangles(grid, point, radius, limit = 60000) {
	const { view, cell, mask, start, items } = grid;
	const reach = Math.ceil(radius / cell);
	if ((2 * reach + 1) ** 3 > 40000) return null;
	const home = point.map((v) => Math.floor(v / cell));
	const seen = new Set();
	const found = [];
	const p = [0, 0, 0];
	const r2 = radius * radius;
	for (let dx = -reach; dx <= reach; dx++) {
		for (let dy = -reach; dy <= reach; dy++) {
			for (let dz = -reach; dz <= reach; dz++) {
				const b = hashCell(home[0] + dx, home[1] + dy, home[2] + dz, mask);
				if (seen.has(b)) continue;
				seen.add(b);
				for (let j = start[b]; j < start[b + 1]; j++) {
					const i = items[j];
					centroid(view, i, p);
					if ((p[0] - point[0]) ** 2 + (p[1] - point[1]) ** 2 + (p[2] - point[2]) ** 2 > r2) continue;
					if (found.length === limit) return null;
					found.push(i);
				}
			}
		}
	}
	return found;
}

/**
 * Where a ray meets a triangle, from either side: the distance along `dir`
 * (Möller and Trumbore), or null when it misses.
 */
export function rayTriangle(origin, dir, a, b, c) {
	const e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
	const e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
	const p = [dir[1] * e2[2] - dir[2] * e2[1], dir[2] * e2[0] - dir[0] * e2[2], dir[0] * e2[1] - dir[1] * e2[0]];
	const det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
	if (Math.abs(det) < 1e-30) return null;
	const s = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
	const u = (s[0] * p[0] + s[1] * p[1] + s[2] * p[2]) / det;
	const q = [s[1] * e1[2] - s[2] * e1[1], s[2] * e1[0] - s[0] * e1[2], s[0] * e1[1] - s[1] * e1[0]];
	const v = (dir[0] * q[0] + dir[1] * q[1] + dir[2] * q[2]) / det;
	const t = (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) / det;
	// A little slack: the pixel under the pointer may graze an edge.
	const slack = 1e-3;
	if (u < -slack || v < -slack || u + v > 1 + slack || t < 0) return null;
	return t;
}
