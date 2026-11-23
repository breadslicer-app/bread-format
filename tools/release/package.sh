#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0
#
# Puts the programs a release build made into archives, each with its license
# notices and a short README, and writes a SHA-256 checksum next to each.
#
#   tools/release/package.sh <platform> <release dir> <out dir>
#
# <platform> is windows-x64, linux-x64 or macos-arm64; <release dir> is the
# cargo target's release folder. Every platform gets `bread`; Windows also gets
# Bread Converter and the Explorer thumbnails. Each archive carries its
# program's own version, from its Cargo.toml.
set -euo pipefail

platform=$1
bin=$2
out=$3
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

version() {
	sed -n 's/^version *= *"\(.*\)"/\1/p' "$root/$1/Cargo.toml" | head -n 1
}

checksum() {
	if command -v sha256sum >/dev/null; then
		(cd "$out" && sha256sum "$1" >"$1.sha256")
	else
		(cd "$out" && shasum -a 256 "$1" >"$1.sha256")
	fi
}

# Zips on Windows, where that is what people open; tar.gz elsewhere, which
# keeps the program's executable bit.
pack() {
	local name=$1
	if [ "$platform" = windows-x64 ]; then
		if command -v 7z >/dev/null; then
			(cd "$work" && 7z a -tzip -mx=9 "$out/$name.zip" "$name" >/dev/null)
		else
			(cd "$work" && powershell -NoProfile -Command "Compress-Archive -Path '$name' -DestinationPath '$(cygpath -w "$out/$name.zip")'")
		fi
		checksum "$name.zip"
	else
		tar -czf "$out/$name.tar.gz" -C "$work" "$name"
		checksum "$name.tar.gz"
	fi
}

exe=
[ "$platform" = windows-x64 ] && exe=.exe

name="bread-$(version crates/bread-cli)-$platform"
mkdir -p "$work/$name"
cp "$bin/bread$exe" "$work/$name/"
cp "$here/README-bread.txt" "$work/$name/README.txt"
cp "$root/LICENSE" "$work/$name/LICENSE.txt"
cp "$root/crates/bread-cli/NOTICE" "$work/$name/NOTICE.txt"
pack "$name"

if [ "$platform" = windows-x64 ]; then
	name="Bread-Converter-$(version apps/converter)-$platform"
	mkdir -p "$work/$name/licenses"
	cp "$bin/bread-converter.exe" "$work/$name/Bread Converter.exe"
	cp "$here/README-converter.txt" "$work/$name/README.txt"
	cp "$root/LICENSE" "$work/$name/licenses/LICENSE.txt"
	cp "$root/apps/converter/NOTICE" "$work/$name/licenses/NOTICE.txt"
	cp "$root/apps/converter/assets/fonts/OFL.txt" "$work/$name/licenses/OFL-NotoSans.txt"
	pack "$name"

	name="bread-thumbnails-$(version crates/bread-thumbnails)-$platform"
	mkdir -p "$work/$name"
	cp "$bin/bread_thumbnails.dll" "$work/$name/"
	cp "$here/README-thumbnails.txt" "$work/$name/README.txt"
	cp "$here/register.cmd" "$here/unregister.cmd" "$work/$name/"
	cp "$root/LICENSE" "$work/$name/LICENSE.txt"
	cp "$root/crates/bread-thumbnails/NOTICE" "$work/$name/NOTICE.txt"
	pack "$name"
fi

ls -l "$out"
