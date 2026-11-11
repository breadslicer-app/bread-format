# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The scene chunk, SCNE (SPEC 5)."""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass

from ._bytes import Fields, Meta
from .errors import Malformed

#: A transform: the 3 x 3 matrix M, row by row, then the translation t, so
#: that a point p goes to M p + t (SPEC 5.2).
Transform = tuple[tuple[float, float, float], tuple[float, float, float], tuple[float, float, float], tuple[float, float, float]]

IDENTITY: Transform = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (0.0, 0.0, 0.0))

GROUP, LEAF, SHARED_LEAF, REFERENCE, SUBTREE = 0, 1, 2, 4, 5


@dataclass(frozen=True)
class Node:
    """A node record. ``kind`` is ``"group"``, ``"leaf"``, ``"shared leaf"``
    or ``"reference"``; ``transform`` is None for a node stored without one,
    which is the identity. Then, by kind: ``children`` (node indices),
    ``mesh`` (a mesh index), ``resource`` (a mesh resource index) or
    ``subtree`` (a subtree index)."""

    kind: str
    meta: Meta | None
    transform: Transform | None
    children: tuple[int, ...] = ()
    mesh: int | None = None
    resource: int | None = None
    subtree: int | None = None

    @property
    def is_leaf(self) -> bool:
        return self.kind in ("leaf", "shared leaf")


@dataclass(frozen=True)
class Scene:
    meta: Meta | None
    mesh_count: int
    mesh_resources: tuple[int, ...]
    nodes: tuple[Node, ...]
    subtrees: tuple[int, ...]
    roots: tuple[int, ...]

    def mesh_of(self, node: int) -> int | None:
        """The mesh index a leaf shows, through the mesh resources for a
        shared leaf, or None for a node that is not a leaf."""
        n = self.nodes[node]
        if n.kind == "leaf":
            return n.mesh
        if n.kind == "shared leaf":
            return self.mesh_resources[n.resource]
        return None

    def placements(self) -> Iterator[tuple[int, int, Transform]]:
        """Every leaf shown, with the mesh it shows and its world transform,
        in the order of the walk of SPEC 5.4. A leaf inside a subtree comes
        once for every reference that reaches it."""
        for root in self.roots:
            yield from self._walk(root, IDENTITY)

    def _walk(self, index: int, parent: Transform) -> Iterator[tuple[int, int, Transform]]:
        node = self.nodes[index]
        world = compose(parent, node.transform or IDENTITY)
        if node.kind == "group":
            for child in node.children:
                yield from self._walk(child, world)
        elif node.kind == "reference":
            yield from self._walk(self.subtrees[node.subtree], world)
        else:
            yield index, self.mesh_of(index), world


def compose(a: Transform, b: Transform) -> Transform:
    """a after b: the transform that takes p to a(b(p)), in f64 (SPEC 5.4)."""
    (a0, a1, a2, at), (b0, b1, b2, bt) = (a[0], a[1], a[2], a[3]), (b[0], b[1], b[2], b[3])
    rows = (a0, a1, a2)
    cols = tuple(zip(b0, b1, b2))
    linear = tuple(tuple(sum(r[k] * c[k] for k in range(3)) for c in cols) for r in rows)
    translation = tuple(sum(r[k] * bt[k] for k in range(3)) + at[i] for i, r in enumerate(rows))
    return (*linear, translation)


def apply(transform: Transform, point: tuple[float, float, float]) -> tuple[float, float, float]:
    """M p + t, in f64."""
    m0, m1, m2, t = transform
    return tuple(sum(row[k] * point[k] for k in range(3)) + t[i] for i, row in enumerate((m0, m1, m2)))


def read_scene(payload: bytes) -> Scene:
    """Reads and checks a scene payload of version 1 (SPEC 5.1 to 5.3)."""
    f = Fields(payload)
    meta = f.meta()
    mesh_count = f.u32()

    resources = []
    for _ in range(f.count(4)):
        mesh = f.u32()
        if mesh >= mesh_count:
            raise Malformed
        resources.append(mesh)

    nodes: list[Node] = []
    subtrees: list[int] = []
    placed: set[int] = set()

    def place(node: int) -> None:
        # Names only what is listed before, and gives each node one place.
        if node >= len(nodes) or node in placed:
            raise Malformed
        placed.add(node)

    # A record takes at least 5 bytes: a subtree's kind and node index.
    for _ in range(f.count(5)):
        kind = f.u8()
        if kind == SUBTREE:
            root = f.u32()
            place(root)
            subtrees.append(root)
            continue
        if kind not in (GROUP, LEAF, SHARED_LEAF, REFERENCE):
            raise Malformed
        node_meta = f.meta()
        transform = _transform(f) if f.flag() else None
        if kind == GROUP:
            children = tuple(f.u32() for _ in range(f.count(4)))
            for child in children:
                place(child)
            nodes.append(Node("group", node_meta, transform, children=children))
        elif kind == LEAF:
            mesh = f.u32()
            if mesh >= mesh_count:
                raise Malformed
            nodes.append(Node("leaf", node_meta, transform, mesh=mesh))
        elif kind == SHARED_LEAF:
            resource = f.u32()
            if resource >= len(resources):
                raise Malformed
            nodes.append(Node("shared leaf", node_meta, transform, resource=resource))
        else:
            subtree = f.u32()
            if subtree >= len(subtrees):
                raise Malformed
            nodes.append(Node("reference", node_meta, transform, subtree=subtree))

    roots = tuple(f.u32() for _ in range(f.count(4)))
    for root in roots:
        place(root)
    f.end()
    return Scene(meta, mesh_count, tuple(resources), tuple(nodes), tuple(subtrees), roots)


def _transform(f: Fields) -> Transform:
    values = [f.f64() for _ in range(12)]
    return (tuple(values[0:3]), tuple(values[3:6]), tuple(values[6:9]), tuple(values[9:12]))
