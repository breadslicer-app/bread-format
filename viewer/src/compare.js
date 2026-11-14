// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Side by side: after STL files are converted, the STL and the .bread file
// shown next to each other with one camera, so anyone can see that nothing was
// lost. The STL is read here straight from its bytes, without the format's
// code; the .bread file is the one just written, opened as any reader opens
// it. Triangles keep their order, so triangle i on one side is triangle i on
// the other. A loupe shows the wireframe around the pointer on both sides,
// each drawn from its own data, and the panel the bits of the triangle under
// it.

import { addWords, busy, bytes, openListeners, refresh, showError, state, t, view3d } from './app.js';
import { convertedListeners } from './convert.js';
import {
	binaryStlTriangles,
	bitsText,
	f32Text,
	meshTriangle,
	nearTriangles,
	rayTriangle,
	stlCorners,
	stlTriangle,
	triangleGrid,
} from './core.js';
import { COLORS, FOV, IDENTITY, loupeSize, transformPoint } from './render.js';

addWords({
	en: {
		compareButton: 'Compare side by side',
		compareTitle: 'Side by side',
		compareIntro:
			'Left, the STL, read straight from the file’s bytes. Right, the .bread file, opened as any reader opens it. One camera moves both.',
		compareHint: 'Point at the model, or tap it, to see that place up close on both sides and the bits of the triangle there.',
		loupe: 'Loupe',
		compareClose: 'Back to one view',
		preparing: 'Preparing the comparison…',
		compareAscii: 'Side by side needs binary STL: an ASCII STL keeps its numbers as text, not as bits.',
		compareTooLarge: 'Too large to show twice in a browser: {count} triangles, and side by side goes up to {max}.',
		compareChanged: 'The STL files are not the ones converted: convert them again.',
		labelStl: 'STL · read from the file’s bytes · {size}',
		labelBread: '.bread · opened as any reader does · {size}',
		triangleOf: 'Triangle {n} of {count}',
		inPart: 'In {name}',
		corner: 'Corner {n}',
		vertexOf: '.bread vertex {n}',
		value: 'Value',
		allSame: 'All nine numbers: the same bits on both sides.',
		someDiffer: 'Some numbers differ: the bits marked do not match.',
		normal:
			'The STL also stores a facet normal, here ({n}). .bread does not: it follows from the corners, in their order.',
		closer: 'The triangles here are too small to draw their edges at this size: zoom in, or make the loupe stronger.',
	},
	es: {
		compareButton: 'Comparar lado a lado',
		compareTitle: 'Lado a lado',
		compareIntro:
			'A la izquierda, el STL, leído directo de los bytes del archivo. A la derecha, el .bread, abierto como lo abre cualquier lector. Una sola cámara mueve los dos.',
		compareHint: 'Apuntá al modelo, o tocalo, para ver ese lugar de cerca en los dos lados y los bits del triángulo que hay ahí.',
		loupe: 'Lupa',
		compareClose: 'Volver a una vista',
		preparing: 'Preparando la comparación…',
		compareAscii: 'Lado a lado necesita STL binario: un STL ASCII guarda sus números como texto, no como bits.',
		compareTooLarge: 'Demasiado grande para mostrarlo dos veces en un navegador: {count} triángulos, y lado a lado llega a {max}.',
		compareChanged: 'Los STL no son los que se convirtieron: convertilos de nuevo.',
		labelStl: 'STL · leído de los bytes del archivo · {size}',
		labelBread: '.bread · abierto como cualquier lector · {size}',
		triangleOf: 'Triángulo {n} de {count}',
		inPart: 'En {name}',
		corner: 'Esquina {n}',
		vertexOf: 'vértice {n} del .bread',
		value: 'Valor',
		allSame: 'Los nueve números: los mismos bits de los dos lados.',
		someDiffer: 'Hay números distintos: los bits marcados no coinciden.',
		normal:
			'El STL guarda además una normal por cara, acá ({n}). El .bread no: sale de las esquinas, en su orden.',
		closer: 'Acá los triángulos son demasiado chicos para dibujar sus aristas a este tamaño: acercate, o subí el aumento.',
	},
});

/** Side by side holds both meshes on the graphics card: past this, it is refused. */
const MAX_TRIANGLES = 8_000_000;
const LOUPE_PX = 220;
const ZOOMS = [4, 8, 16, 32, 64];

const lang = document.documentElement.lang;
const number = new Intl.NumberFormat(lang);
const $ = (id) => document.getElementById(id);

const cmp = {
	on: false,
	/** Per part: the STL's bytes, its triangles, its first triangle's number, its grid. */
	stls: [],
	counts: [],
	bases: [],
	grids: [],
	stlBytes: 0,
	zoom: 8,
	/** Where the loupe is: the pointer, in page coordinates. */
	pointer: null,
	dragging: false,
	/** The wireframe drawn now: around `center`, `radius` wide; null when there was too much. */
	lines: null,
	shown: null,
};

const sub = (p, q) => [p[0] - q[0], p[1] - q[1], p[2] - q[2]];

/** The part and triangle of a number given by `pick`. */
function locate(id) {
	for (let part = 0; part < cmp.bases.length; part++) {
		if (id >= cmp.bases[part] && id < cmp.bases[part] + cmp.counts[part]) return { part, tri: id - cmp.bases[part] };
	}
	return null;
}

const breadMesh = (part) => state.file.meshes[state.file.placements[part].mesh];

/** The edges of triangles, two ends a line, each end x, y, z and a colour. */
function edges(count, corner) {
	const out = new Float32Array(count * 36);
	let at = 0;
	for (let j = 0; j < count; j++) {
		for (let c = 0; c < 3; c++) {
			for (const end of [c, (c + 1) % 3]) {
				out.set(corner(j, end), at);
				out.set(COLORS.edge, at + 3);
				at += 6;
			}
		}
	}
	return out;
}

function stlLines(part, tris) {
	const stl = cmp.stls[part];
	const view = new DataView(stl.buffer, stl.byteOffset, stl.byteLength);
	return edges(tris.length, (j, c) => {
		const at = 84 + 50 * tris[j] + 12 + 12 * c;
		return [view.getFloat32(at, true), view.getFloat32(at + 4, true), view.getFloat32(at + 8, true)];
	});
}

function breadLines(part, tris) {
	const mesh = breadMesh(part);
	const matrix = state.file.placements[part].matrix;
	return edges(tris.length, (j, c) => {
		const v = 3 * mesh.triangles[3 * tris[j] + c];
		return transformPoint(matrix, [mesh.vertices[v], mesh.vertices[v + 1], mesh.vertices[v + 2]]);
	});
}

function joined(arrays) {
	const out = new Float32Array(arrays.reduce((sum, a) => sum + a.length, 0));
	let at = 0;
	for (const a of arrays) {
		out.set(a, at);
		at += a.length;
	}
	return out;
}

/** The wireframe around `center`, on both sides, unless the one drawn already covers it. */
function wireframe(center, radius) {
	const renderer = view3d();
	const drawn = cmp.lines;
	// Drawn with too many triangles, it is looked for again once the loupe is closer.
	const enough = drawn && (!drawn.tooMany || radius >= (drawn.radius / 1.6) * 0.95);
	if (
		enough &&
		drawn.radius >= radius &&
		drawn.radius <= radius * 2.5 &&
		Math.hypot(...center.map((v, k) => v - drawn.center[k])) <= drawn.radius - radius
	) {
		return;
	}
	const wide = radius * 1.6;
	const near = cmp.grids.map((grid) => nearTriangles(grid, center, wide, Math.floor(60000 / cmp.grids.length)));
	if (near.some((n) => n === null)) {
		cmp.lines = { center, radius: wide, tooMany: true };
		renderer.setLoupeLines([null, null]);
	} else {
		cmp.lines = { center, radius: wide, tooMany: false };
		renderer.setLoupeLines([
			joined(near.map((tris, part) => stlLines(part, tris))),
			joined(near.map((tris, part) => breadLines(part, tris))),
		]);
	}
	$('compare-closer').hidden = !cmp.lines.tooMany;
}

/** Looks under the pointer again: the loupe, its wireframe, the lit triangle and the panel. */
function update() {
	const renderer = view3d();
	if (!cmp.on || !renderer) return;
	const p = cmp.pointer;
	const id = p && !cmp.dragging ? renderer.pick(p[0], p[1]) : -1;
	const ray = p ? renderer.rayAt(p[0], p[1]) : null;
	const where = id >= 0 && ray ? locate(id) : null;
	if (!where) {
		renderer.setLoupe(null);
		cmp.lines = null;
		show(null);
		return;
	}
	const { part, tri } = where;
	const stl = stlTriangle(cmp.stls[part], tri);
	const [a, b, c] = stl.corners.map((corner) => corner.values);
	const d = rayTriangle(ray.origin, ray.dir, a, b, c);
	const target = d === null ? [0, 1, 2].map((k) => (a[k] + b[k] + c[k]) / 3) : ray.origin.map((o, k) => o + d * ray.dir[k]);
	// What half the loupe spans at that distance.
	const r = renderer.viewRects()[ray.view];
	const size = loupeSize(LOUPE_PX, r);
	const distance = Math.hypot(...target.map((v, k) => v - ray.origin[k]));
	const half = (distance * Math.tan(FOV / 2) * (size / r.h)) / cmp.zoom;
	renderer.setLoupe({ at: ray.at, size: LOUPE_PX, zoom: cmp.zoom, target });
	// The edges of the triangle there, in the loupe's pixels: under a few, the
	// wireframe would be a solid mass of lines, so it asks to come closer.
	const edge = (Math.hypot(...sub(a, b)) + Math.hypot(...sub(b, c)) + Math.hypot(...sub(c, a))) / 3;
	if (edge * (size / (2 * half)) < 5) {
		cmp.lines = null;
		renderer.setLoupeLines([null, null]);
		$('compare-closer').hidden = false;
	} else {
		wireframe(target, half * Math.SQRT2);
	}
	if (cmp.shown?.part !== part || cmp.shown?.tri !== tri) {
		const mesh = breadMesh(part);
		const matrix = state.file.placements[part].matrix;
		const lit = meshTriangle(mesh, tri).corners.map((corner) => transformPoint(matrix, corner.values));
		renderer.setHighlight([Float32Array.from([a, b, c].flat()), Float32Array.from(lit.flat())]);
		show({ part, tri });
	}
}

let scheduled = false;
function schedule() {
	if (scheduled) return;
	scheduled = true;
	requestAnimationFrame(() => {
		scheduled = false;
		update();
	});
}

// --- The panel ------------------------------------------------------------------

function cell(text, className) {
	const el = document.createElement('span');
	el.textContent = text;
	if (className) el.className = className;
	return el;
}

/** The triangle under the pointer, both sides, bit by bit; or the hint when there is none. */
function show(where) {
	cmp.shown = where;
	const box = $('compare-triangle');
	if (!where) {
		const hint = document.createElement('p');
		hint.className = 'note';
		hint.textContent = t('compareHint');
		box.replaceChildren(hint);
		return;
	}
	const { part, tri } = where;
	const stl = stlTriangle(cmp.stls[part], tri);
	const bread = meshTriangle(breadMesh(part), tri);
	const head = document.createElement('p');
	head.className = 'triangle-head';
	head.textContent = t('triangleOf', { n: number.format(tri + 1), count: number.format(cmp.counts[part]) });
	const parts = [head];
	if (cmp.stls.length > 1) {
		const name = document.createElement('p');
		name.className = 'note';
		name.textContent = t('inPart', { name: state.file.scene.nodes[state.file.placements[part].leaf].meta?.name ?? part + 1 });
		parts.push(name);
	}
	let same = true;
	for (let c = 0; c < 3; c++) {
		const title = document.createElement('p');
		title.className = 'corner-head';
		title.textContent = `${t('corner', { n: c + 1 })} · ${t('vertexOf', { n: number.format(bread.corners[c].vertex) })}`;
		const grid = document.createElement('div');
		grid.className = 'bits';
		grid.append(cell(''), cell(t('value'), 'bits-head'), cell('STL', 'bits-head'), cell('.bread', 'bits-head'), cell(''));
		for (let k = 0; k < 3; k++) {
			const left = stl.corners[c].bits[k];
			const right = bread.corners[c].bits[k];
			const equal = left === right;
			same &&= equal;
			const value = equal
				? f32Text(stl.corners[c].values[k])
				: `${f32Text(stl.corners[c].values[k])} ≠ ${f32Text(bread.corners[c].values[k])}`;
			grid.append(
				cell('xyz'[k], 'axis'),
				cell(value, 'value'),
				cell(bitsText(left), equal ? 'hex' : 'hex differ'),
				cell(bitsText(right), equal ? 'hex' : 'hex differ'),
				cell(equal ? '✓' : '✗', equal ? 'mark ok' : 'mark differ'),
			);
		}
		parts.push(title, grid);
	}
	const verdict = document.createElement('p');
	verdict.className = same ? 'check ok' : 'check failed';
	verdict.textContent = same ? t('allSame') : t('someDiffer');
	const normal = document.createElement('p');
	normal.className = 'note';
	normal.textContent = t('normal', { n: stl.normal.values.map(f32Text).join(', ') });
	box.replaceChildren(...parts, verdict, normal);
}

function panel() {
	const section = $('compare');
	const h = document.createElement('h2');
	h.textContent = t('compareTitle');
	const intro = document.createElement('p');
	intro.className = 'note';
	intro.textContent = t('compareIntro');
	const zoom = document.createElement('div');
	zoom.className = 'zoom';
	zoom.append(cell(t('loupe'), 'zoom-label'));
	for (const z of ZOOMS) {
		const b = document.createElement('button');
		b.type = 'button';
		b.className = 'button small';
		b.textContent = `×${z}`;
		b.setAttribute('aria-pressed', String(z === cmp.zoom));
		b.addEventListener('click', () => {
			cmp.zoom = z;
			for (const other of zoom.querySelectorAll('button')) other.setAttribute('aria-pressed', String(other === b));
			cmp.lines = null;
			schedule();
		});
		zoom.append(b);
	}
	const closer = document.createElement('p');
	closer.id = 'compare-closer';
	closer.className = 'note';
	closer.textContent = t('closer');
	closer.hidden = true;
	const triangle = document.createElement('div');
	triangle.id = 'compare-triangle';
	const close = document.createElement('button');
	close.type = 'button';
	close.className = 'button wide';
	close.textContent = t('compareClose');
	close.addEventListener('click', stop);
	section.replaceChildren(h, intro, zoom, closer, triangle, close);
	section.hidden = false;
	show(null);
}

// --- On the canvas ----------------------------------------------------------------

/** The labels over each view and the frames around the loupes, where the renderer drew them. */
function overlays() {
	const renderer = view3d();
	const labels = [$('label-stl'), $('label-bread')];
	const frames = [$('loupe-0'), $('loupe-1')];
	const views = cmp.on ? renderer.viewRects() : [];
	const loupes = cmp.on ? renderer.loupeRects() : [];
	labels.forEach((el, v) => {
		el.hidden = !views[v];
		if (views[v]) {
			el.style.left = `${views[v].x + 10}px`;
			el.style.top = `${views[v].y + 10}px`;
		}
	});
	frames.forEach((el, v) => {
		el.hidden = !loupes[v];
		if (loupes[v]) {
			el.style.left = `${loupes[v].x}px`;
			el.style.top = `${loupes[v].y}px`;
			el.style.width = `${loupes[v].w}px`;
			el.style.height = `${loupes[v].h}px`;
		}
	});
}

const canvas = $('view');
let down = null;
canvas.addEventListener('pointerdown', (e) => {
	if (!cmp.on) return;
	down = { x: e.clientX, y: e.clientY, type: e.pointerType };
	cmp.dragging = true;
});
canvas.addEventListener('pointerup', (e) => {
	if (!cmp.on) return;
	cmp.dragging = false;
	// A mouse leaves the loupe where it is; a tap puts it where the finger was.
	if (e.pointerType === 'mouse' || (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) < 8)) {
		cmp.pointer = [e.clientX, e.clientY];
	}
	down = null;
	schedule();
});
canvas.addEventListener('pointercancel', () => {
	cmp.dragging = false;
	down = null;
});
canvas.addEventListener('pointermove', (e) => {
	if (!cmp.on || e.pointerType !== 'mouse' || e.buttons !== 0) return;
	cmp.pointer = [e.clientX, e.clientY];
	schedule();
});
canvas.addEventListener('pointerleave', (e) => {
	if (!cmp.on || e.pointerType !== 'mouse' || e.buttons !== 0) return;
	cmp.pointer = null;
	schedule();
});

// --- In and out -------------------------------------------------------------------

state.hooks.push((items) => {
	const renderer = view3d();
	if (!cmp.on || !renderer) return items;
	// The STL side shows what the .bread side shows: the same parts, hidden or lit alike.
	renderer.setCompare(
		items
			.filter((item) => typeof item.mesh === 'number' && item.placement < cmp.stls.length)
			.map((item) => ({
				mesh: `stl-${item.placement}`,
				matrix: IDENTITY,
				color: item.color,
				visible: item.visible,
				base: cmp.bases[item.placement],
			})),
	);
	return items;
});

async function start(files) {
	const renderer = view3d();
	const file = state.file;
	if (!renderer || !file) return;
	busy(t('preparing'));
	// A frame for the message to show before the work starts.
	await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve)));
	try {
		const stls = await Promise.all(files.map(async (f) => new Uint8Array(await f.arrayBuffer())));
		const counts = stls.map(binaryStlTriangles);
		if (counts.some((c) => c === null)) throw new Error(t('compareAscii'));
		const total = counts.reduce((sum, c) => sum + c, 0);
		if (total > MAX_TRIANGLES) {
			throw new Error(t('compareTooLarge', { count: number.format(total), max: number.format(MAX_TRIANGLES) }));
		}
		const fits =
			file.placements.length === stls.length &&
			counts.every((c, part) => file.meshes[file.placements[part].mesh].triangles.length === 3 * c);
		if (!fits) throw new Error(t('compareChanged'));
		stop();
		cmp.stls = stls;
		cmp.counts = counts;
		cmp.bases = counts.map((_, part) => counts.slice(0, part).reduce((sum, c) => sum + c, 0));
		cmp.stlBytes = stls.reduce((sum, s) => sum + s.length, 0);
		stls.forEach((stl, part) => renderer.addCorners(`stl-${part}`, stlCorners(stl)));
		cmp.grids = stls.map(triangleGrid);
		cmp.on = true;
		$('label-stl').textContent = t('labelStl', { size: bytes(cmp.stlBytes) });
		$('label-bread').textContent = t('labelBread', { size: bytes(file.size) });
		panel();
		for (const button of document.querySelectorAll('.compare-button')) button.hidden = true;
		renderer.onChange = schedule;
		renderer.onDraw = overlays;
		// The hook above gives the STL side its items.
		refresh();
		busy(null);
	} catch (err) {
		busy(null);
		showError(err.message ?? String(err));
	}
}

function stop() {
	const renderer = view3d();
	if (renderer) {
		renderer.setCompare(null);
		renderer.setLoupe(null);
		cmp.stls.forEach((_, part) => renderer.removeMesh(`stl-${part}`));
	}
	Object.assign(cmp, { on: false, stls: [], counts: [], bases: [], grids: [], pointer: null, lines: null, shown: null });
	$('compare').hidden = true;
	for (const button of document.querySelectorAll('.compare-button')) button.hidden = false;
	overlays();
}

window.addEventListener('keydown', (e) => {
	if (e.key === 'Escape' && cmp.on) stop();
});

// A new file, opened or converted, ends the comparison of the last one.
openListeners.push(() => {
	if (cmp.on || !$('compare').hidden) stop();
});

convertedListeners.push((files, result) => {
	if (!result.verified || !view3d()) return;
	const button = document.createElement('button');
	button.type = 'button';
	button.className = 'button wide compare-button';
	button.textContent = t('compareButton');
	button.addEventListener('click', () => start(files));
	$('converted').append(button);
});
