# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Placing the meshes (SPEC 5.4) and reading paint (SPEC 10)."""

from __future__ import annotations

import struct
import unittest
from pathlib import Path

from bread_format import apply, read_file
from bread_format.scene import read_scene

CONFORMANCE = Path(__file__).resolve().parents[2] / "conformance"


def read(name: str):
    return read_file((CONFORMANCE / "valid" / f"{name}.bread").read_bytes())


def moved(x: float, y: float, z: float) -> bytes:
    """Node fields with no metadata and a translation."""
    return b"\x00\x01" + struct.pack("<12d", 1, 0, 0, 0, 1, 0, 0, 0, 1, x, y, z)


class Placing(unittest.TestCase):
    def test_a_leaf_is_placed_by_its_own_transform_and_its_parents(self):
        placed = [(leaf, transform[3]) for leaf, _, transform in read("two-parts").placements()]
        self.assertEqual(placed, [(0, (0.0, 0.0, 0.0)), (1, (0.0, 0.0, 1.5))])

    def test_a_shared_mesh_is_placed_by_each_leaf(self):
        bread = read("shared-mesh")
        (_, first, a), (_, second, b) = bread.placements()
        self.assertIs(first, second)
        self.assertEqual(apply(a, (1.0, 0.0, 0.0)), (3.0, 0.0, 0.0))
        self.assertEqual(apply(b, (1.0, 0.0, 0.0)), (-3.25, 0.6, 0.001))

    def test_each_reference_places_one_more_copy_of_its_subtree(self):
        payload = (
            b"\x00"  # no metadata
            + struct.pack("<I", 1)  # one mesh
            + struct.pack("<I", 0)  # no mesh resources
            + struct.pack("<I", 4)  # four records
            + b"\x01" + moved(1, 0, 0) + struct.pack("<I", 0)  # node 0: a leaf of mesh 0
            + b"\x05" + struct.pack("<I", 0)  # subtree 0: node 0
            + b"\x04" + moved(0, 2, 0) + struct.pack("<I", 0)  # node 1: a reference to it
            + b"\x04\x00\x00" + struct.pack("<I", 0)  # node 2: another, not moved
            + struct.pack("<3I", 2, 1, 2)  # roots: nodes 1 and 2
        )
        scene = read_scene(payload)
        placed = [(leaf, mesh, transform[3]) for leaf, mesh, transform in scene.placements()]
        self.assertEqual(placed, [(0, 0, (1.0, 2.0, 0.0)), (0, 0, (1.0, 0.0, 0.0))])


class Paint(unittest.TestCase):
    def test_paint_gives_a_value_per_triangle_of_the_mesh_it_was_made_on(self):
        bread = read("surface-paint")
        cube = bread.meshes[0]
        detail, keep_off = bread.surface
        self.assertEqual(list(detail.values(cube)), [255, 255] + [0] * 10)
        self.assertEqual(list(keep_off.values(cube)), [0] * 4 + [128, 128] + [0] * 6)

    def test_paint_on_another_mesh_is_treated_as_absent(self):
        bread = read("surface-paint")
        triangle = read("triangle").meshes[0]
        self.assertIsNone(bread.surface[0].values(triangle))


if __name__ == "__main__":
    unittest.main()
