# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests of the supports builder, without Blender: needs Python 3.11+ and
numpy.

    python -m unittest discover blender/tests -p "test_supports.py"
"""

import json
import math
import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bread_import import supports  # noqa: E402


def joint(i, x, y, z, d=1.2):
    return {"id": i, "pos": {"x": x, "y": y, "z": z}, "diameter": d}


# One support: a base on the plate, a trunk of two stretches, a tip on the
# model, a branch from a knot on the trunk, a leaf from that knot, and a
# brace to a second knot.
DOCUMENT = {
    "recipe": {"options": {"raft": "solid", "raftThicknessMm": 0.5}},
    "state": {
        "roots": {
            "r": {
                "id": "r",
                "transform": {"pos": {"x": 0, "y": 0, "z": 0}, "rot": {"x": 0, "y": 0, "z": 0, "w": 1}},
                "diameter": 5.0,
                "diskHeight": 0.25,
                "coneHeight": 0.0,
                "joinConeMm": 0.4,
            }
        },
        "trunks": {
            "t": {
                "id": "t",
                "rootId": "r",
                "baseDiameterMm": 1.1,
                "segments": [
                    {"id": "s1", "diameter": 1.1, "topJoint": joint("j1", 0, 0, 10)},
                    {"id": "s2", "diameter": 1.1, "topJoint": joint("j2", 1, 0, 14), "bottomJoint": joint("j1", 0, 0, 10)},
                ],
                "contactCone": {
                    "pos": {"x": 1.5, "y": 0, "z": 16.4},
                    "normal": {"x": -0.2, "y": 0, "z": -0.98},
                    "profile": {"contactDiameterMm": 0.24, "bodyDiameterMm": 1.1, "lengthMm": 2.3,
                                "type": "disk", "diskThicknessMm": 0.1},
                    "socketJointId": "j2",
                },
            }
        },
        "knots": {
            "k": {"id": "k", "pos": {"x": 0, "y": 0, "z": 6}, "diameter": 1.2},
            "k2": {"id": "k2", "pos": {"x": 0, "y": 0, "z": 3}, "diameter": 1.2},
        },
        "branches": {
            "b": {
                "id": "b",
                "parentKnotId": "k",
                "segments": [{"id": "s3", "diameter": 1.1, "topJoint": joint("j3", -3, 0, 9)}],
                "contactCone": {
                    "pos": {"x": -3.5, "y": 0, "z": 11.4},
                    "normal": {"x": 0, "y": 0, "z": -1},
                    "profile": {"contactDiameterMm": 0.3, "bodyDiameterMm": 1.1, "lengthMm": 2.3,
                                "type": "disk", "diskThicknessMm": 0.1},
                    "socketJointId": "j3",
                },
            }
        },
        "leaves": {
            "l": {
                "id": "l",
                "parentKnotId": "k",
                "contactCone": {
                    "pos": {"x": 2, "y": 2, "z": 9},
                    "normal": {"x": -0.4, "y": -0.4, "z": -0.8},
                    "profile": {"contactDiameterMm": 0.24, "bodyDiameterMm": 0.77, "lengthMm": 3.4,
                                "type": "disk", "diskThicknessMm": 0.1},
                },
            }
        },
        "braces": {"x": {"id": "x", "startKnotId": "k2", "endKnotId": "k", "profile": {"diameter": 0.7}}},
    },
}


def closed_and_outward(vertices, triangles):
    """Whether every shape is closed, its triangles agree on their way
    round, and they face out (a positive volume)."""
    directed = set()
    for a, b, c in triangles.tolist():
        for e in ((a, b), (b, c), (c, a)):
            if e in directed:
                return False
            directed.add(e)
    if any((b, a) not in directed for a, b in directed):
        return False
    p = vertices.astype(np.float64)[triangles]
    return np.einsum("ij,ij->i", p[:, 0], np.cross(p[:, 1], p[:, 2])).sum() > 0


class Build(unittest.TestCase):
    def test_every_piece_gets_a_closed_shape(self):
        v, t = supports.build(DOCUMENT)
        self.assertEqual(v.dtype, np.float32)
        self.assertEqual(t.dtype, np.uint32)
        self.assertLess(int(t.max()), len(v))
        self.assertTrue(closed_and_outward(v, t))

    def test_shapes_follow_the_document(self):
        s = supports.shapes(DOCUMENT)
        # disk + join cone, two trunk stretches, tip disk and cone, branch
        # stretch, branch tip (2), leaf tip (2), brace.
        self.assertEqual(len(s.cylinders), 2 + 2 + 2 + 1 + 2 + 2 + 1)
        # three joints and two knots
        self.assertEqual(len(s.balls), 5)
        ends = [tuple(np.round(c[1], 6)) for c in s.cylinders]
        # The trunk starts on top of its base and the tip reaches its socket.
        starts = [tuple(np.round(c[0], 6)) for c in s.cylinders]
        self.assertIn((0.0, 0.0, 0.25), starts)
        self.assertIn((1.0, 0.0, 14.0), ends)
        # The leaf ends at its knot.
        self.assertIn((0.0, 0.0, 6.0), ends)

    def test_the_tip_is_as_wide_as_its_profile(self):
        s = supports.shapes(DOCUMENT)
        tip = next(c for c in s.cylinders if np.allclose(c[0], [1.5, 0, 16.4]))
        self.assertAlmostEqual(tip[2], 0.12)
        self.assertAlmostEqual(tip[3], 0.12)

    def test_the_raft_lies_under_the_bases(self):
        v, _ = supports.build(DOCUMENT)
        self.assertAlmostEqual(float(v[:, 2].min()), 0.0)
        flat = v[np.isclose(v[:, 2], 0.0)]
        self.assertAlmostEqual(float(np.abs(flat[:, :2]).max()), 2.5, places=4)

    def test_no_raft_when_the_document_has_none(self):
        doc = json.loads(json.dumps(DOCUMENT))
        doc["recipe"]["options"]["raft"] = "none"
        self.assertIsNone(supports.shapes(doc).raft)

    def test_detail_sets_the_sides(self):
        low, _ = supports.build(DOCUMENT, "LOW")
        high, _ = supports.build(DOCUMENT, "HIGH")
        self.assertGreater(len(high), 2 * len(low))

    def test_a_document_with_missing_pieces_still_builds(self):
        doc = {"state": {"trunks": {"t": {"rootId": "nowhere", "segments": [{"diameter": 1.0}]}},
                         "braces": {"b": {"startKnotId": "a", "endKnotId": "b"}}}}
        v, t = supports.build(doc)
        self.assertEqual(len(t), 0)
        with self.assertRaises(supports.SupportsError):
            supports.build({"recipe": {}})

    def test_only_documents_it_knows(self):
        raw = json.dumps(DOCUMENT).encode()
        self.assertEqual(supports.read_document("df", 2, raw)["state"].keys(), DOCUMENT["state"].keys())
        with self.assertRaises(supports.SupportsError):
            supports.read_document("other", 2, raw)
        with self.assertRaises(supports.SupportsError):
            supports.read_document("df", 3, raw)
        with self.assertRaises(supports.SupportsError):
            supports.read_document("df", 2, b"\xff")

    def test_turned_bases_stand_on_their_axis(self):
        doc = json.loads(json.dumps(DOCUMENT))
        half = math.sqrt(0.5)
        doc["state"]["roots"]["r"]["transform"]["rot"] = {"x": half, "y": 0, "z": 0, "w": half}
        s = supports.shapes(doc)
        disk = s.cylinders[0]
        self.assertTrue(np.allclose(disk[1] - disk[0], [0, -0.25, 0]))


if __name__ == "__main__":
    unittest.main()
