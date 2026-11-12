// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Support previews (SPEC.md 9): a support leaf is stored with an empty mesh,
// and its preview is the mesh of the supports as they were when the file was
// written. Each is drawn where its leaf is, in Breadslicer's support colour,
// with a switch to hide them all.

import { addWords, openListeners, refresh, state, t, view3d } from './app.js';
import { hiddenParts } from './parts.js';
import { COLORS } from './render.js';

addWords({
	en: {
		supportsTitle: 'Supports',
		showSupports: 'Show the supports',
		supportsNote: 'A preview of the supports, as they were when the file was saved.',
	},
	es: {
		supportsTitle: 'Soportes',
		showSupports: 'Mostrar los soportes',
		supportsNote: 'Una vista previa de los soportes, como estaban al guardar el archivo.',
	},
});

let show = true;

state.hooks.push((items) => {
	const file = state.file;
	if (!file?.preview) return items;
	const hidden = hiddenParts();
	const supports = [];
	file.preview.forEach((k, i) => {
		if (k < 0) return;
		supports.push({
			mesh: `preview-${k}`,
			matrix: file.placements[i].matrix,
			color: COLORS.support,
			visible: show && !hidden.has(i),
			placement: i,
		});
	});
	return [...items, ...supports];
});

openListeners.push((file) => {
	const section = document.getElementById('supports');
	section.hidden = file.previews.length === 0;
	if (file.previews.length === 0) return;
	const renderer = view3d();
	if (renderer) {
		file.previews.forEach((p, k) => renderer.addMesh(`preview-${k}`, p.mesh.vertices, p.mesh.triangles));
	}
	show = true;
	const h = document.createElement('h2');
	h.textContent = t('supportsTitle');
	const box = document.createElement('input');
	box.type = 'checkbox';
	box.checked = true;
	box.addEventListener('change', () => {
		show = box.checked;
		refresh();
	});
	const label = document.createElement('label');
	label.className = 'toggle';
	label.append(box, document.createTextNode(t('showSupports')));
	const note = document.createElement('p');
	note.className = 'note';
	note.textContent = t('supportsNote');
	section.replaceChildren(h, label, note);
	refresh();
	// Framed again, now with the supports in it.
	renderer?.frame();
});
