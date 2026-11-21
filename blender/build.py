# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Builds the Blender extension: the .bread reader as a native library, then
the extension's zip in blender/dist.

    python blender/build.py [--install]

Needs Rust and Blender 4.2 or later. BLENDER names Blender's executable when
it is not in the default place. --install also installs the zip in
Blender's "user_default" repository and enables it."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
SOURCE = HERE / "bread_import"
DIST = HERE / "dist"

LIBRARY = {"win32": ("bread_wasm.dll", "bread.dll"),
           "linux": ("libbread_wasm.so", "libbread.so"),
           "darwin": ("libbread_wasm.dylib", "libbread.dylib")}


def blender() -> str:
    if os.environ.get("BLENDER"):
        return os.environ["BLENDER"]
    found = shutil.which("blender")
    if found:
        return found
    if sys.platform == "win32":
        base = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Blender Foundation"
        candidates = sorted(base.glob("Blender */blender.exe"))
        if candidates:
            return str(candidates[-1])
    sys.exit("Blender not found: set BLENDER to its executable")


def main() -> None:
    built, shipped = LIBRARY[sys.platform]
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target"))
    env = dict(os.environ)
    if sys.platform == "win32":
        # No dependency on the Visual C++ runtime being installed.
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -C target-feature=+crt-static").strip()
    subprocess.run(["cargo", "build", "--release", "-p", "bread-wasm"], cwd=REPO, env=env, check=True)

    lib = SOURCE / "lib"
    lib.mkdir(exist_ok=True)
    shutil.copy2(target / "release" / built, lib / shipped)
    shutil.copy2(REPO / "crates" / "bread-wasm" / "NOTICE", lib / "NOTICE")
    shutil.copy2(REPO / "LICENSE", lib / "LICENSE")

    DIST.mkdir(exist_ok=True)
    exe = blender()
    subprocess.run(
        [exe, "--factory-startup", "--command", "extension", "build",
         "--source-dir", str(SOURCE), "--output-dir", str(DIST)],
        check=True,
    )
    zips = sorted(DIST.glob("bread_import-*.zip"), key=lambda p: p.stat().st_mtime)
    print(f"built {zips[-1]}")
    if "--install" in sys.argv[1:]:
        subprocess.run(
            [exe, "--command", "extension", "install-file", "--repo", "user_default",
             "--enable", str(zips[-1])],
            check=True,
        )


if __name__ == "__main__":
    main()
