// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Puts the viewer together in dist/: its own files and fonts, the JavaScript package as
// bread.js, both WebAssembly modules, and the notices that go with them. The
// folder is the whole page, ready to be served as static files from any
// address. Build the package first: `npm run build` in bindings/js.

import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const pkg = join(root, 'bindings/js');
const out = join(here, 'dist');

for (const wasm of ['bread.wasm', 'bread-write.wasm']) {
	if (!existsSync(join(pkg, 'dist', wasm))) {
		console.error(`bindings/js/dist/${wasm} is missing: run \`npm run build\` in bindings/js first`);
		process.exit(1);
	}
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out);
// Its own files, the fonts folder with its licence included.
cpSync(join(here, 'src'), out, { recursive: true });
copyFileSync(join(pkg, 'src/index.js'), join(out, 'bread.js'));
for (const wasm of ['bread.wasm', 'bread-write.wasm']) copyFileSync(join(pkg, 'dist', wasm), join(out, wasm));
// The modules hold zstd and meshoptimizer: their notices go with them.
copyFileSync(join(pkg, 'dist/NOTICE'), join(out, 'NOTICE'));
copyFileSync(join(root, 'LICENSE'), join(out, 'LICENSE'));
console.log(`wrote ${readdirSync(out).length} files to viewer/dist`);
