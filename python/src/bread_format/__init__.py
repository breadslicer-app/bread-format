# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Reads .bread files: triangle meshes, kept bit for bit, and the scene that
places them.

Written from the specification alone (SPEC.md), with the Python standard
library only, as a second reader of the format::

    from bread_format import read_file

    with open("model.bread", "rb") as f:
        bread = read_file(f.read())
    for leaf, mesh, transform in bread.placements():
        ...
"""

from ._bytes import Meta
from .errors import BreadError
from .file import BreadFile, read_file
from .mesh import Mesh, read_mesh
from .scene import IDENTITY, Node, Scene, Transform, apply, compose
from .slicer import Entry, Paint, fingerprint_of

__all__ = [
    "IDENTITY",
    "BreadError",
    "BreadFile",
    "Entry",
    "Mesh",
    "Meta",
    "Node",
    "Paint",
    "Scene",
    "Transform",
    "apply",
    "compose",
    "fingerprint_of",
    "read_file",
    "read_mesh",
]
