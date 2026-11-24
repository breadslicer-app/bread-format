#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0
#
# Fails when a built program still holds one of the given strings, such as the
# checkout's path or the building user's name: the builds remap them away.
#
#   tools/release/check-paths.sh <file> <string>...
set -euo pipefail

file=$1
shift
found=0
for s in "$@"; do
	[ -n "$s" ] || continue
	if grep -a -F -i -q -- "$s" "$file"; then
		echo "$file holds \"$s\"" >&2
		found=1
	fi
done
exit $found
