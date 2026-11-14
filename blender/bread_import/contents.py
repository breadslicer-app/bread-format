# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""What a .bread file holds, as the importer needs it: the scene placed
(SPEC 5.4), the meshes, and which leaves are models, hollows and supports
(SPEC 7.3, 9). No Blender here, so it can be tested without it."""

from __future__ import annotations

import struct
from dataclasses import dataclass, field

import numpy as np

from . import native
from .native import BreadError

# Past this many nodes walked, each copy counted, a scene is refused: a small
# file can ask for a great many copies of copies.
PLACEMENT_LIMIT = 1_000_000


@dataclass(frozen=True)
class Entry:
    """A SUPP or HOLW entry of version 2 (SPEC 7.3). ``part`` is the support
    leaf in SUPP and the original's mesh index in HOLW."""

    model: int
    part: int
    generator: str
    version: int
    document: bytes


def read_entries(payload: bytes, chunk: str) -> list[Entry]:
    """The entries of a SUPP or HOLW payload of version 2."""

    at = 0

    def take(n: int) -> bytes:
        nonlocal at
        if at + n > len(payload):
            raise BreadError("malformed", chunk)
        out = payload[at : at + n]
        at += n
        return out

    def u32() -> int:
        return struct.unpack("<I", take(4))[0]

    count = u32()
    if count * 20 > len(payload) - 4:
        raise BreadError("malformed", chunk)
    entries = []
    for _ in range(count):
        model, part = u32(), u32()
        try:
            generator = take(u32()).decode("utf-8")
        except UnicodeDecodeError:
            raise BreadError("malformed", chunk) from None
        version = u32()
        entries.append(Entry(model, part, generator, version, take(u32())))
    if at != len(payload):
        raise BreadError("malformed", chunk)
    return entries


@dataclass
class Placement:
    leaf: int
    mesh: int
    matrix: np.ndarray  # 4 × 4, f64, p' = M p


def _own(node: native.Node) -> np.ndarray:
    m = np.identity(4)
    if node.transform is not None:
        t = node.transform
        m[:3, :3] = np.array(t[:9]).reshape(3, 3)
        m[:3, 3] = t[9:12]
    return m


def place(scene: native.Scene) -> list[Placement]:
    """Every leaf shown and its world transform, walking down from each root
    in order (SPEC 5.4)."""
    placed = []
    stack = [(root, np.identity(4)) for root in reversed(scene.roots)]
    walked = 0
    while stack:
        walked += 1
        if walked > PLACEMENT_LIMIT:
            raise BreadError("malformed", "SCNE")
        index, parent = stack.pop()
        node = scene.nodes[index]
        world = parent if node.transform is None else parent @ _own(node)
        if node.kind == "group":
            stack.extend((child, world) for child in reversed(node.children))
        elif node.kind == "reference":
            stack.append((scene.subtrees[node.subtree], world))
        else:
            placed.append(Placement(index, scene.leaf_mesh(index), world))
    return placed


@dataclass
class Contents:
    chunks: list[native.Chunk]
    scene: native.Scene
    placements: list[Placement]
    supports: list[Entry] = field(default_factory=list)
    hollows: list[Entry] = field(default_factory=list)
    _mesh_chunks: list[native.Chunk] = field(default_factory=list)
    _meshes: dict[int, native.Mesh] = field(default_factory=dict)

    @property
    def support_leaves(self) -> set[int]:
        return {e.part for e in self.supports}

    @property
    def hollow_models(self) -> dict[int, int]:
        """Each hollowed model's leaf, mapped to its original's mesh."""
        return {e.model: e.part for e in self.hollows}

    def mesh(self, index: int) -> native.Mesh:
        if index not in self._meshes:
            self._meshes[index] = native.decode_mesh(self._mesh_chunks[index])
        return self._meshes[index]

    def models(self) -> list[Placement]:
        """The leaves shown that are not supports."""
        leaves = self.support_leaves
        return [p for p in self.placements if p.leaf not in leaves]

    def previews(self) -> dict[int, native.Mesh]:
        if not any(c.tag == "SPRV" for c in self.chunks):
            return {}
        empty = [len(self.mesh(i).vertices) == 0 and len(self.mesh(i).triangles) == 0
                 for i in range(len(self._mesh_chunks))]
        return native.decode_previews(self.chunks, empty)


def read(data: bytes) -> Contents:
    chunks = native.read_chunks(data)
    scene = native.decode_scene(chunks)
    contents = Contents(chunks, scene, place(scene))
    contents._mesh_chunks = [c for c in chunks if c.tag == "MESH"]
    nodes = len(scene.nodes)
    for c in chunks:
        # Version 1 was only written while the format was a draft: skipped.
        if c.tag in ("SUPP", "HOLW") and c.version == 2:
            entries = read_entries(c.payload, c.tag)
            for e in entries:
                if e.model >= nodes or scene.nodes[e.model].kind not in ("leaf", "shared leaf"):
                    raise BreadError("malformed", c.tag)
            if c.tag == "SUPP":
                contents.supports = entries
            else:
                contents.hollows = entries
    return contents
