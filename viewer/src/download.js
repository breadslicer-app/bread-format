// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Download as STL, for programs that do not read .bread yet: the parts shown,
// or one part, as binary STL in world positions, each vertex moved by its
// leaf's transform in f64 and then rounded to f32.

import { addWords, openListeners, state, t } from './app.js';
import { toBinaryStl } from './core.js';
import { hiddenParts, partName } from './parts.js';

addWords({
	en: {
		downloadStl: 'Download as STL',
		downloadShown: 'The parts shown, as one STL',
		downloadPart: 'Download {name} as STL',
		stlNote: 'In world positions, for programs that do not read .bread yet.',
	},
	es: {
		downloadStl: 'Descargar como STL',
		downloadShown: 'Las piezas a la vista, en un STL',
		downloadPart: 'Descargar {name} como STL',
		stlNote: 'En sus posiciones finales, para programas que todavía no leen .bread.',
	},
});

/** Saves `bytes` as a file named `name`. */
function save(name, bytes) {
	const url = URL.createObjectURL(new Blob([bytes], { type: 'model/stl' }));
	const a = document.createElement('a');
	a.href = url;
	a.download = name;
	document.body.append(a);
	a.click();
	a.remove();
	setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

const base = () => (state.name ?? 'scene.bread').replace(/\.bread$/i, '');

function stlOf(indices) {
	const file = state.file;
	return toBinaryStl(indices.map((i) => ({ mesh: file.meshes[file.placements[i].mesh], matrix: file.placements[i].matrix })));
}

openListeners.push((file) => {
	const section = document.getElementById('parts');
	// Each part's own download, at the end of its row.
	for (const li of section.querySelectorAll('li[data-placement]')) {
		const i = Number(li.dataset.placement);
		const name = partName(file, i);
		const button = document.createElement('button');
		button.type = 'button';
		button.className = 'icon';
		button.title = t('downloadPart', { name });
		button.setAttribute('aria-label', t('downloadPart', { name }));
		button.textContent = 'STL';
		button.addEventListener('click', (e) => {
			e.preventDefault();
			save(`${name.replace(/\.(stl|bread)$/i, '')}.stl`, stlOf([i]));
		});
		li.querySelector('.part').append(button);
	}
	const all = document.createElement('button');
	all.type = 'button';
	all.className = 'button wide';
	all.textContent = t('downloadShown');
	all.addEventListener('click', () => {
		const hidden = hiddenParts();
		const shown = file.placements.map((_, i) => i).filter((i) => !hidden.has(i));
		if (shown.length > 0) save(`${base()}.stl`, stlOf(shown));
	});
	const note = document.createElement('p');
	note.className = 'note';
	note.textContent = t('stlNote');
	const block = document.createElement('div');
	block.className = 'download';
	block.append(all, note);
	section.append(block);
});
