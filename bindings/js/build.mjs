// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// Builds crates/bread-wasm to WebAssembly and copies the module to
// dist/bread.wasm, with its NOTICE.
//
// Needs the wasm32-unknown-unknown Rust target and a clang that targets
// WebAssembly: set WASI_SDK_PATH to a WebAssembly SDK, or set
// CC_wasm32_unknown_unknown and CXX_wasm32_unknown_unknown yourself.

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const crate = join(root, 'crates/bread-wasm');
const env = { ...process.env };

const sdk = env.WASI_SDK_PATH;
if (sdk) {
	const bin = join(sdk, 'bin');
	const exe = process.platform === 'win32' ? '.exe' : '';
	env.CC_wasm32_unknown_unknown ??= join(bin, `clang${exe}`);
	env.CXX_wasm32_unknown_unknown ??= join(bin, `clang++${exe}`);
	// The cc crate looks for llvm-ar on the PATH.
	env.PATH = `${env.PATH}${delimiter}${bin}`;
}
if (!env.CC_wasm32_unknown_unknown) {
	console.error('Set WASI_SDK_PATH, or CC_wasm32_unknown_unknown, to a clang that targets WebAssembly.');
	process.exit(1);
}
// See wasm/meshopt-math.h.
const include = `-include ${join(crate, 'wasm/meshopt-math.h').replaceAll('\\', '/')}`;
env.CFLAGS_wasm32_unknown_unknown = include;
env.CXXFLAGS_wasm32_unknown_unknown = include;

execFileSync(
	'cargo',
	['build', '-p', 'bread-wasm', '--profile', 'wasm', '--target', 'wasm32-unknown-unknown'],
	{ cwd: root, env, stdio: 'inherit' },
);

const target = env.CARGO_TARGET_DIR ? resolve(root, env.CARGO_TARGET_DIR) : join(root, 'target');
const built = join(target, 'wasm32-unknown-unknown/wasm/bread_wasm.wasm');
if (!existsSync(built)) {
	console.error(`cargo did not write ${built}`);
	process.exit(1);
}
mkdirSync(join(here, 'dist'), { recursive: true });
copyFileSync(built, join(here, 'dist/bread.wasm'));
copyFileSync(join(crate, 'NOTICE'), join(here, 'dist/NOTICE'));
console.log('wrote dist/bread.wasm');
