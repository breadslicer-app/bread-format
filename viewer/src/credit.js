// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Who made the file (SPEC.md 12): its title, its designer, its license and
// where it comes from, first in the panel, so whoever prints a model can find
// whoever made it. Everything the file says is shown as text, and its source
// is a link only when it is an http or https address.

import { addWords, openListeners, t } from './app.js';
import { creditOf } from './core.js';

addWords({
	en: {
		creditTitle: 'Made by',
		by: 'Designed by {name}',
		license: 'License',
		source: 'Source',
		copyright: 'Copyright',
	},
	es: {
		creditTitle: 'Hecho por',
		by: 'Diseñado por {name}',
		license: 'Licencia',
		source: 'Fuente',
		copyright: 'Derechos',
	},
});

openListeners.push((file) => {
	const section = document.getElementById('credit');
	const credit = creditOf(file.metadata ?? []);
	const shown = ['title', 'designer', 'license', 'source', 'copyright', 'description'].some(
		(key) => credit[key] !== undefined,
	);
	section.hidden = !shown;
	if (!shown) {
		section.replaceChildren();
		return;
	}
	const parts = [];
	if (credit.title !== undefined) {
		const title = document.createElement('p');
		title.className = 'credit-title';
		title.textContent = credit.title;
		parts.push(title);
	} else {
		const h = document.createElement('h2');
		h.textContent = t('creditTitle');
		parts.push(h);
	}
	if (credit.designer !== undefined) {
		const by = document.createElement('p');
		by.className = 'credit-by';
		by.textContent = t('by', { name: credit.designer });
		parts.push(by);
	}
	const rows = [];
	if (credit.license !== undefined) rows.push([t('license'), credit.license]);
	if (credit.source !== undefined) {
		let value = credit.source;
		if (credit.link !== null) {
			value = document.createElement('a');
			value.href = credit.link;
			value.target = '_blank';
			value.rel = 'noopener noreferrer nofollow';
			value.textContent = credit.source;
		}
		rows.push([t('source'), value]);
	}
	if (credit.copyright !== undefined) rows.push([t('copyright'), credit.copyright]);
	if (rows.length > 0) {
		const dl = document.createElement('dl');
		dl.className = 'facts';
		for (const [k, v] of rows) {
			const dt = document.createElement('dt');
			dt.textContent = k;
			const dd = document.createElement('dd');
			dd.append(v);
			dl.append(dt, dd);
		}
		parts.push(dl);
	}
	if (credit.description !== undefined) {
		const note = document.createElement('p');
		note.className = 'note';
		note.textContent = credit.description;
		parts.push(note);
	}
	section.replaceChildren(...parts);
});
