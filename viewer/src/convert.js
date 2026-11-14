// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// STL to .bread: STL files dropped or picked become one .bread file, each a
// part named after its file. The worker writes it, reads it back and checks
// every mesh, and every corner of a binary STL, bit for bit, before the page
// offers it; then the page opens it, so what is shown is what was written.

import { addWords, ask, busy, bytes, fileHandlers, openBytes, showError, t } from './app.js';

addWords({
	en: {
		convert: 'STL to .bread',
		emptyConvert: 'Or drop STL files to turn them into one .bread file, checked bit for bit.',
		converting: 'Turning {count} STL into .bread…',
		converted: 'Converted',
		fromStl: 'STL',
		toBread: '.bread',
		ratio: '{x}× smaller',
		verified: 'Checked: read back, every corner comes out bit for bit as in the STL.',
		notVerified: 'The check failed: this file does not give back the STL exactly. It is not offered.',
		download: 'Download {name}',
		took: 'Took',
		cannotConvert: 'This STL cannot be converted ({name}): {message}.',
		partsCount: '{count} parts',
	},
	es: {
		convert: 'STL a .bread',
		emptyConvert: 'O soltá archivos STL para convertirlos en un solo .bread, verificado bit a bit.',
		converting: 'Convirtiendo {count} STL a .bread…',
		converted: 'Convertido',
		fromStl: 'STL',
		toBread: '.bread',
		ratio: '{x}× más chico',
		verified: 'Verificado: al releerlo, cada esquina sale bit a bit igual que en el STL.',
		notVerified: 'La verificación falló: este archivo no devuelve el STL exacto. No se ofrece.',
		download: 'Descargar {name}',
		took: 'Tardó',
		cannotConvert: 'No se puede convertir este STL ({name}): {message}.',
		partsCount: '{count} piezas',
	},
});

const decimal = new Intl.NumberFormat(document.documentElement.lang, {
	maximumFractionDigits: 1,
	minimumFractionDigits: 1,
});

const stem = (name) => name.replace(/\.stl$/i, '');

/** The name of the file made from `files`: the STL's own, or the first's for a kit. */
export function breadName(files) {
	const first = stem(files[0].name) || 'model';
	return files.length === 1 ? `${first}.bread` : `${first}-kit.bread`;
}

let lastUrl = null;

function showResult(name, result) {
	const section = document.getElementById('converted');
	const h = document.createElement('h2');
	h.textContent = t('converted');
	const dl = document.createElement('dl');
	dl.className = 'facts';
	const rows = [
		[t('fromStl'), `${bytes(result.stlBytes)}${result.parts.length > 1 ? ` · ${t('partsCount', { count: result.parts.length })}` : ''}`],
		[t('toBread'), `${bytes(result.file.length)} · ${t('ratio', { x: decimal.format(result.stlBytes / result.file.length) })}`],
		[t('took'), `${Math.round(result.ms)} ms`],
	];
	for (const [k, v] of rows) {
		const dt = document.createElement('dt');
		dt.textContent = k;
		const dd = document.createElement('dd');
		dd.textContent = v;
		dl.append(dt, dd);
	}
	const check = document.createElement('p');
	check.className = result.verified ? 'check ok' : 'check failed';
	check.textContent = result.verified ? t('verified') : t('notVerified');
	section.replaceChildren(h, dl, check);
	if (result.verified) {
		if (lastUrl) URL.revokeObjectURL(lastUrl);
		lastUrl = URL.createObjectURL(new Blob([result.file], { type: 'application/octet-stream' }));
		const a = document.createElement('a');
		a.className = 'button primary wide';
		a.href = lastUrl;
		a.download = name;
		a.textContent = t('download', { name });
		section.append(a);
	}
	section.hidden = false;
}

/** Converts STL files in the worker, shows the result, and opens it. */
export async function convertFiles(files) {
	busy(t('converting', { count: files.length }));
	const read = await Promise.all(files.map(async (f) => ({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
	const name = breadName(files);
	let result;
	try {
		result = await ask(
			{ type: 'convert', files: read, name: files.length > 1 ? stem(name.replace(/\.bread$/, '')) : undefined },
			read.map((f) => f.bytes.buffer),
		);
	} catch (err) {
		busy(null);
		showError(t('cannotConvert', { name: files.length === 1 ? files[0].name : files.map((f) => f.name).join(', '), message: err.message }));
		return;
	}
	// Opened from a copy: the worker takes the bytes it reads, and the
	// download keeps the file.
	await openBytes(name, result.file.slice());
	showResult(name, result);
	for (const listener of convertedListeners) listener(files, result);
}

/** Called after each conversion, with the STL files and what came out. */
export const convertedListeners = [];

fileHandlers.push({
	accepts: (files) => files.length >= 1 && files.every((f) => /\.stl$/i.test(f.name)),
	handle: convertFiles,
});

document.getElementById('convert').addEventListener('change', (e) => {
	const files = [...e.target.files];
	e.target.value = '';
	if (files.length > 0) convertFiles(files);
});
