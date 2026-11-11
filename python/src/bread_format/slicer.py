# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The chunks a slicer adds: the application chunks PROJ, SUPP and HOLW
(SPEC 7.3), support previews, SPRV (SPEC 9), and surface paint, SURF
(SPEC 10). Only their envelopes: what a document holds belongs to the program
that wrote it."""

from __future__ import annotations

import hashlib
import struct
from dataclasses import dataclass

from ._bytes import Fields
from .errors import Malformed


@dataclass(frozen=True)
class Entry:
    """A SUPP or HOLW entry. ``part`` is the support leaf's node index in
    SUPP, and the original's mesh index in HOLW."""

    model: int
    part: int
    generator: str
    version: int
    document: bytes


def read_entries(payload: bytes) -> list[Entry]:
    """The entries of a SUPP or HOLW payload of version 2."""
    f = Fields(payload)
    entries = []
    for _ in range(f.count(20)):
        model = f.u32()
        part = f.u32()
        generator = f.text()
        version = f.u32()
        entries.append(Entry(model, part, generator, version, bytes(f.byte_string())))
    f.end()
    return entries


@dataclass(frozen=True)
class Preview:
    """A support preview: the support leaf, and its mesh's version 1 payload."""

    leaf: int
    mesh: bytes


def read_previews(payload: bytes) -> list[Preview]:
    """The previews of a SPRV payload of version 1, their meshes still
    encoded."""
    f = Fields(payload)
    previews = []
    for _ in range(f.count(8)):
        leaf = f.u32()
        previews.append(Preview(leaf, bytes(f.byte_string())))
    f.end()
    return previews


@dataclass(frozen=True)
class Paint:
    """A SURF entry: one mask over the triangles of one leaf's mesh."""

    leaf: int
    kind: int  # 0 detail, 1 supports must touch, 2 supports must not touch
    version: int
    document: bytes

    def values(self, mesh) -> bytes | None:
        """The value of every triangle of ``mesh``, 0 to 255, or None when the
        entry is to be treated as absent (SPEC 10.2): a reserved kind, a
        document version other than 1, a malformed document, or one painted
        on another mesh."""
        if self.kind > 2 or self.version != 1:
            return None
        try:
            f = Fields(self.document)
            triangle_count = f.u32()
            fingerprint = bytes(f.take(48))
            runs = [(f.u32(), f.u8()) for _ in range(f.count(5))]
            f.end()
        except Malformed:
            return None
        if triangle_count != mesh.triangle_count or fingerprint != fingerprint_of(mesh):
            return None
        if sum(length for length, _ in runs) != triangle_count:
            return None
        return b"".join(bytes([value]) * length for length, value in runs)


def read_paint(payload: bytes) -> list[Paint]:
    """The entries of a SURF payload of version 1."""
    f = Fields(payload)
    entries = []
    for _ in range(f.count(13)):
        leaf = f.u32()
        kind = f.u8()
        version = f.u32()
        entries.append(Paint(leaf, kind, version, bytes(f.byte_string())))
    f.end()
    return entries


def fingerprint_of(mesh) -> bytes:
    """The SHA-384 digest that names a mesh (SPEC 10.1)."""
    digest = hashlib.sha384()
    digest.update(struct.pack("<Q", mesh.vertex_count))
    digest.update(mesh.vertex_bytes())
    digest.update(struct.pack("<Q", mesh.triangle_count))
    digest.update(mesh.triangle_bytes())
    return digest.digest()
