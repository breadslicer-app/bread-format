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
import { homedir } from 'node:os';
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

// Paths on the building machine end up in the modules, in panic messages and
// in C assertions. Replace the two that hold them, the Cargo home and this
// checkout, so a published module names no user folder.
const prefixes = [
	[env.CARGO_HOME ?? join(homedir(), '.cargo'), '/cargo'],
	[root, '/bread-format'],
];
const rustflags = env.CARGO_ENCODED_RUSTFLAGS
	? env.CARGO_ENCODED_RUSTFLAGS.split('\x1f')
	: (env.RUSTFLAGS ?? '').split(/\s+/).filter(Boolean);
for (const [from, to] of prefixes) rustflags.push(`--remap-path-prefix=${from}=${to}`);
env.CARGO_ENCODED_RUSTFLAGS = rustflags.join('\x1f');
delete env.RUSTFLAGS;
for (const [from, to] of prefixes) {
	for (const path of new Set([from, from.replaceAll('\\', '/')])) {
		env.CFLAGS_wasm32_unknown_unknown += ` -ffile-prefix-map=${path}=${to}`;
		env.CXXFLAGS_wasm32_unknown_unknown += ` -ffile-prefix-map=${path}=${to}`;
	}
}

const target = env.CARGO_TARGET_DIR ? resolve(root, env.CARGO_TARGET_DIR) : join(root, 'target');
const built = join(target, 'wasm32-unknown-unknown/wasm/bread_wasm.wasm');
mkdirSync(join(here, 'dist'), { recursive: true });

// Two modules: bread.wasm reads, and bread-write.wasm also reads STL and
// writes .bread, which takes zstd's compressor and meshoptimizer's encoders
// and more than doubles it. A page that only shows files loads the first.
for (const [name, features] of [
	['bread.wasm', []],
	['bread-write.wasm', ['--features', 'write']],
]) {
	execFileSync(
		'cargo',
		['build', '-p', 'bread-wasm', '--profile', 'wasm', '--target', 'wasm32-unknown-unknown', ...features],
		{ cwd: root, env, stdio: 'inherit' },
	);
	if (!existsSync(built)) {
		console.error(`cargo did not write ${built}`);
		process.exit(1);
	}
	copyFileSync(built, join(here, 'dist', name));
	console.log(`wrote dist/${name}`);
}
copyFileSync(join(crate, 'NOTICE'), join(here, 'dist/NOTICE'));
