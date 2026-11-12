// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// The viewer page: opens a .bread file dropped or picked, and shows its
// models placed as the scene says, on a millimetre grid. The files are read
// in a worker (worker.js) and never leave the browser.

import { COLORS, Renderer } from './render.js';

// --- Words, in English and Spanish ---------------------------------------------

const WORDS = {
	en: {
		title: 'viewer',
		open: 'Open .bread',
		emptyLead: 'Drop a .bread file here to look at it.',
		private: 'It all runs in your browser: your files never leave your computer.',
		controls: 'Drag to turn · right drag or shift to move · wheel to zoom · double click to frame.',
		dropHere: 'Drop to open',
		reading: 'Reading {name}…',
		file: 'File',
		size: 'Size',
		parts: 'Parts shown',
		triangles: 'Triangles',
		vertices: 'Vertices',
		asStl: 'As binary STL',
		smaller: '{x}× smaller',
		openedIn: 'Read in',
		notBread: 'Only .bread and STL files open here.',
		noWebgl: 'This browser cannot draw 3D (WebGL2): the file can still be read, but not shown.',
		refused: 'This file cannot be opened: {message}.',
	},
	es: {
		title: 'visor',
		open: 'Abrir .bread',
		emptyLead: 'Soltá un archivo .bread acá para verlo.',
		private: 'Todo pasa en tu navegador: tus archivos nunca salen de tu computadora.',
		controls: 'Arrastrá para girar · clic derecho o shift para mover · rueda para acercar · doble clic para encuadrar.',
		dropHere: 'Soltá para abrir',
		reading: 'Leyendo {name}…',
		file: 'Archivo',
		size: 'Tamaño',
		parts: 'Piezas a la vista',
		triangles: 'Triángulos',
		vertices: 'Vértices',
		asStl: 'Como STL binario',
		smaller: '{x}× más chico',
		openedIn: 'Leído en',
		notBread: 'Acá solo se abren archivos .bread y STL.',
		noWebgl: 'Este navegador no puede dibujar 3D (WebGL2): el archivo se puede leer, pero no mostrar.',
		refused: 'No se puede abrir este archivo: {message}.',
	},
};

const lang = (navigator.languages ?? [navigator.language]).some((l) => l?.toLowerCase().startsWith('es'))
	? 'es'
	: 'en';
document.documentElement.lang = lang;

/** A word in the page's language, with `{name}` filled in from `fill`. */
export function t(key, fill = {}) {
	const text = WORDS[lang][key] ?? WORDS.en[key] ?? key;
	return text.replace(/\{(\w+)\}/g, (_, k) => String(fill[k] ?? ''));
}

function translatePage() {
	for (const el of document.querySelectorAll('[data-text]')) el.textContent = t(el.dataset.text);
}

/** More words, from a part of the page that brings its own: `{ en: {...}, es: {...} }`. */
export function addWords(more) {
	for (const [l, words] of Object.entries(more)) Object.assign(WORDS[l], words);
	translatePage();
}

translatePage();

const number = new Intl.NumberFormat(lang);
const decimal = new Intl.NumberFormat(lang, { maximumFractionDigits: 1, minimumFractionDigits: 1 });

/** Bytes as the page shows them: decimal units, 1 MB = 1,000,000 bytes. */
export function bytes(n) {
	if (n < 1e3) return `${number.format(n)} B`;
	if (n < 1e6) return `${decimal.format(n / 1e3)} KB`;
	if (n < 1e9) return `${decimal.format(n / 1e6)} MB`;
	return `${decimal.format(n / 1e9)} GB`;
}

// --- The worker ------------------------------------------------------------------

const worker = new Worker(new URL('worker.js', import.meta.url), { type: 'module' });
let nextId = 0;
const waiting = new Map();
worker.onmessage = ({ data }) => {
	const done = waiting.get(data.id);
	waiting.delete(data.id);
	if (data.error) done.reject(data.error);
	else done.resolve(data.result);
};

/** Asks the worker for something; `transfer` hands buffers over. */
export function ask(message, transfer = []) {
	const id = nextId++;
	return new Promise((resolve, reject) => {
		waiting.set(id, { resolve, reject });
		worker.postMessage({ ...message, id }, transfer);
	});
}

// --- The page --------------------------------------------------------------------

const $ = (id) => document.getElementById(id);

let renderer = null;
try {
	renderer = new Renderer($('view'));
} catch {
	$('view').hidden = true;
}

/** What is open: the file as the worker read it. */
export const state = { name: null, file: null, hooks: [] };

export function showError(message) {
	const box = $('error');
	box.textContent = message;
	box.hidden = false;
}

export function busy(text) {
	$('error').hidden = true;
	$('busy-text').textContent = text;
	$('busy').hidden = text === null;
}

/** The colour of each placement, and whether it shows: hooks may change them. */
export function items() {
	const file = state.file;
	return file.placements.map((p, i) => ({
		mesh: p.mesh,
		matrix: p.matrix,
		color: COLORS.model,
		visible: true,
		placement: i,
	}));
}

/** Draws what is open again, after a hook changed what shows. */
export function refresh() {
	if (!renderer || !state.file) return;
	let list = items();
	for (const hook of state.hooks) list = hook(list) ?? list;
	renderer.setItems(list);
}

function fileFacts(file) {
	const triangles = file.placements.reduce((sum, p) => sum + file.meshes[p.mesh].triangles.length / 3, 0);
	const vertices = file.placements.reduce((sum, p) => sum + file.meshes[p.mesh].vertices.length / 3, 0);
	const stl = 84 + 50 * triangles;
	const rows = [
		[t('file'), state.name],
		[t('size'), bytes(file.size)],
		[t('parts'), number.format(file.placements.length)],
		[t('triangles'), number.format(triangles)],
		[t('vertices'), number.format(vertices)],
		[t('asStl'), `${bytes(stl)} · ${t('smaller', { x: decimal.format(stl / file.size) })}`],
		[t('openedIn'), `${number.format(Math.round(file.ms))} ms`],
	];
	const dl = document.createElement('dl');
	dl.className = 'facts';
	for (const [k, v] of rows) {
		const dt = document.createElement('dt');
		dt.textContent = k;
		const dd = document.createElement('dd');
		dd.textContent = v;
		dl.append(dt, dd);
	}
	const section = $('file-info');
	section.replaceChildren(dl);
}

/** Opens `.bread` bytes: reads them in the worker and shows them. */
export async function openBytes(name, data) {
	busy(t('reading', { name }));
	// What the converter last made belongs to the file it made.
	$('converted').hidden = true;
	try {
		const file = await ask({ type: 'open', bytes: data }, [data.buffer]);
		state.name = name;
		state.file = file;
		if (renderer) {
			renderer.clear();
			file.meshes.forEach((m, i) => renderer.addMesh(i, m.vertices, m.triangles));
			refresh();
			renderer.frame();
		} else {
			showError(t('noWebgl'));
		}
		$('empty').hidden = true;
		$('panel').hidden = false;
		fileFacts(file);
		for (const listener of openListeners) listener(file);
		busy(null);
	} catch (err) {
		busy(null);
		showError(t('refused', { message: err.message ?? String(err) }));
	}
}

/** Called with each file opened. */
export const openListeners = [];

/** Handlers for dropped or picked files, by extension; the viewer opens `.bread`. */
export const fileHandlers = [
	{
		accepts: (files) => files.length >= 1 && files.every((f) => /\.bread$/i.test(f.name)),
		handle: async (files) => openBytes(files[0].name, new Uint8Array(await files[0].arrayBuffer())),
	},
];

async function take(fileList) {
	const files = [...fileList];
	if (files.length === 0) return;
	const handler = fileHandlers.find((h) => h.accepts(files));
	if (handler) await handler.handle(files);
	else showError(t('notBread'));
}

$('open').addEventListener('change', (e) => {
	take(e.target.files);
	e.target.value = '';
});

let dragDepth = 0;
window.addEventListener('dragenter', (e) => {
	e.preventDefault();
	dragDepth++;
	$('drop').hidden = false;
});
window.addEventListener('dragleave', () => {
	dragDepth = Math.max(0, dragDepth - 1);
	if (dragDepth === 0) $('drop').hidden = true;
});
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('drop', (e) => {
	e.preventDefault();
	dragDepth = 0;
	$('drop').hidden = true;
	take(e.dataTransfer.files);
});

// A file named in the address, `?file=name.bread`, next to the page: for
// links to an example.
const named = new URLSearchParams(location.search).get('file');
if (named && /^[\w.-]+\.bread$/.test(named)) {
	fetch(new URL(named, location.href))
		.then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(r.statusText))))
		.then((b) => openBytes(named, new Uint8Array(b)))
		.catch((err) => showError(t('refused', { message: err.message })));
}
