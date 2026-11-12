// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// The parts panel: every leaf the scene shows, with its name, its triangles
// and its size in millimetres, each shown or hidden on its own and lit in the
// amber of selection while the pointer is over it.

import { addWords, openListeners, refresh, state, t as translate } from './app.js';
import { COLORS } from './render.js';

addWords({
	en: {
		partsTitle: 'Parts',
		part: 'Part {n}',
		showAll: 'Show all',
		hideAll: 'Hide all',
		partTriangles: '{count} triangles',
		show: 'Show {name}',
	},
	es: {
		partsTitle: 'Piezas',
		part: 'Pieza {n}',
		showAll: 'Mostrar todas',
		hideAll: 'Ocultar todas',
		partTriangles: '{count} triángulos',
		show: 'Mostrar {name}',
	},
});

const lang = document.documentElement.lang;
const number = new Intl.NumberFormat(lang);
const mm = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 });

/** Which placements are hidden, and which one the pointer is over. */
const view = { hidden: new Set(), hover: null };

/** A leaf's name: its own, else its mesh's, else its number. */
export function partName(file, index) {
	const placement = file.placements[index];
	return (
		file.scene.nodes[placement.leaf].meta?.name ??
		file.meshes[placement.mesh].meta?.name ??
		translate('part', { n: index + 1 })
	);
}

state.hooks.push((items) =>
	items.map((item) => ({
		...item,
		visible: item.visible && !view.hidden.has(item.placement),
		color: view.hover === item.placement ? COLORS.selected : item.color,
	})),
);

function list(file) {
	const section = document.getElementById('parts');
	const h = document.createElement('h2');
	h.textContent = translate('partsTitle');
	const all = document.createElement('button');
	all.type = 'button';
	all.className = 'link';
	const syncAll = () => {
		all.textContent = translate(view.hidden.size === file.placements.length ? 'showAll' : 'hideAll');
	};
	all.addEventListener('click', () => {
		const hideAll = view.hidden.size < file.placements.length;
		view.hidden = hideAll ? new Set(file.placements.map((_, i) => i)) : new Set();
		for (const box of section.querySelectorAll('input[type=checkbox]')) box.checked = !hideAll;
		syncAll();
		refresh();
	});
	const head = document.createElement('div');
	head.className = 'section-head';
	head.append(h, all);

	const ul = document.createElement('ul');
	ul.className = 'parts';
	file.placements.forEach((placement, i) => {
		const li = document.createElement('li');
		const name = partName(file, i);
		const box = document.createElement('input');
		box.type = 'checkbox';
		box.checked = true;
		box.setAttribute('aria-label', translate('show', { name }));
		box.addEventListener('change', () => {
			if (box.checked) view.hidden.delete(i);
			else view.hidden.add(i);
			syncAll();
			refresh();
		});
		const label = document.createElement('div');
		label.className = 'part-name';
		label.textContent = name;
		label.title = name;
		const size = file.sizes[i];
		const detail = document.createElement('div');
		detail.className = 'part-detail';
		detail.textContent = `${translate('partTriangles', {
			count: number.format(file.meshes[placement.mesh].triangles.length / 3),
		})} · ${size.map((v) => mm.format(v)).join(' × ')} mm`;
		const text = document.createElement('label');
		text.className = 'part';
		text.append(box, label, detail);
		li.append(text);
		li.addEventListener('pointerenter', () => {
			view.hover = i;
			refresh();
		});
		li.addEventListener('pointerleave', () => {
			if (view.hover === i) view.hover = null;
			refresh();
		});
		li.dataset.placement = String(i);
		ul.append(li);
	});
	syncAll();
	section.replaceChildren(head, ul);
}

openListeners.push((file) => {
	view.hidden = new Set();
	view.hover = null;
	list(file);
	refresh();
});
