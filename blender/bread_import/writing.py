# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""What the exporter writes, without Blender: a scene chunk payload that
places each part (SPEC 5.1), a metadata chunk payload (SPEC 12.1), and the
file, written by the bread-format crate (native.write_file), which checks
both as a reader does and encodes the meshes."""

from __future__ import annotations

import struct
from dataclasses import dataclass

import numpy as np

from . import native

# The subject of a metadata entry about the whole file.
WHOLE_FILE = 0xFFFFFFFF

# The registered keys whose value is text (SPEC 12.1).
TEXT_KEYS = ("title", "description", "designer", "copyright", "license", "source", "created", "generator")


@dataclass
class Part:
    """A leaf of the scene: a name, the index of the mesh it shows, and its
    transform from the mesh's space to the scene's, as a 4 × 4 matrix, or
    None for the identity."""

    name: str
    mesh: int
    matrix: np.ndarray | None


@dataclass
class MeshData:
    vertices: np.ndarray  # (n, 3) float32, written bit for bit
    triangles: np.ndarray  # (m, 3) uint32, in order, first corner first


def _text(out: bytearray, value: str | None) -> None:
    if value is None:
        out += b"\x00"
        return
    data = value.encode("utf-8")
    out += b"\x01" + struct.pack("<I", len(data)) + data


def encode_scene(parts: list[Part], mesh_count: int) -> bytes:
    """A scene chunk payload of version 1: each part a leaf with its name and
    transform, every leaf a root, in order."""
    out = bytearray()
    out += b"\x00"  # the scene itself has no name
    out += struct.pack("<I", mesh_count)
    out += struct.pack("<I", 0)  # no mesh resources
    out += struct.pack("<I", len(parts))
    for part in parts:
        out += b"\x01"  # a leaf
        out += b"\x01"  # with a name
        _text(out, part.name)
        _text(out, None)  # and no comment
        if part.matrix is None:
            out += b"\x00"
        else:
            m = np.asarray(part.matrix, dtype=np.float64)
            out += b"\x01" + struct.pack("<12d", *m[:3, :3].ravel(), *m[:3, 3])
        out += struct.pack("<I", part.mesh)
    out += struct.pack("<I", len(parts))
    for i in range(len(parts)):
        out += struct.pack("<I", i)
    return bytes(out)


def encode_metadata(entries: list[tuple[int | None, str, str]]) -> bytes:
    """A metadata chunk payload of version 1 of text entries: (node, key,
    value), the node None for the whole file."""
    out = bytearray(struct.pack("<I", len(entries)))
    for node, key, value in entries:
        data = value.encode("utf-8")
        out += struct.pack("<I", WHOLE_FILE if node is None else node)
        out += struct.pack("<I", len(key)) + key.encode("ascii")
        out += struct.pack("<I", len(data)) + data
    return bytes(out)


def write(meshes: list[MeshData], parts: list[Part], metadata: list[tuple[int | None, str, str]]) -> bytes:
    """The bytes of a .bread file of these meshes, placed by these parts,
    with this metadata. Raises native.BreadError when the crate refuses
    them."""
    payload = encode_scene(parts, len(meshes))
    return native.write_file(
        [(m.vertices, m.triangles) for m in meshes],
        payload,
        encode_metadata(metadata) if metadata else None,
    )
