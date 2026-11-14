# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Imports every valid conformance file in Blender and checks that each mesh
arrives bit for bit: the same vertex bits and the same triangles, in order.
Run it with Blender, after blender/build.py:

    blender -b --factory-startup --python-exit-code 1 --python blender/tests/test_import.py
"""

import hashlib
import json
import sys
import unittest
from pathlib import Path

import bpy
import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))

import bread_import  # noqa: E402

VALID = REPO / "conformance" / "valid"


def clear():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj)
    for me in list(bpy.data.meshes):
        bpy.data.meshes.remove(me)
    for coll in list(bpy.data.collections):
        bpy.data.collections.remove(coll)


def mesh_hashes(me):
    co = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get("co", co)
    idx = np.empty(len(me.loops), dtype=np.int32)
    me.loops.foreach_get("vertex_index", idx)
    sizes = np.empty(len(me.polygons), dtype=np.int32)
    me.polygons.foreach_get("loop_total", sizes)
    assert np.all(sizes == 3)
    return (
        hashlib.sha256(co.astype("<f4").tobytes()).hexdigest(),
        hashlib.sha256(idx.astype("<u4").tobytes()).hexdigest(),
    )


def run(path, **options):
    clear()
    result = bpy.ops.import_scene.bread(filepath=str(path), **options)
    return result, [o for o in bpy.data.objects if o.type == "MESH"]


class ImportConformance(unittest.TestCase):
    def test_every_valid_file_arrives_bit_for_bit(self):
        files = sorted(VALID.glob("*.bread"))
        self.assertTrue(files)
        for path in files:
            with self.subTest(path.name):
                case = json.loads(path.with_suffix(".json").read_text())
                expected = {(m["vertices_sha256"], m["triangles_sha256"]) for m in case["meshes"]}
                result, objects = run(path)
                self.assertEqual(result, {"FINISHED"})
                models = [o for o in objects if not o.name.endswith("supports")]
                self.assertTrue(models)
                for obj in models:
                    self.assertIn(mesh_hashes(obj.data), expected, obj.name)

    def test_shared_meshes_stay_shared(self):
        _, objects = run(VALID / "shared-mesh.bread")
        self.assertGreater(len(objects), len({o.data.name for o in objects}))

    def test_hollow_off_brings_the_original(self):
        case = json.loads((VALID / "slicer-data.json").read_text())
        original = case["meshes"][2]
        _, objects = run(VALID / "slicer-data.bread", import_hollow=False)
        hashes = [mesh_hashes(o.data) for o in objects]
        self.assertIn((original["vertices_sha256"], original["triangles_sha256"]), hashes)
        _, objects = run(VALID / "slicer-data.bread", import_hollow=True)
        hashes = [mesh_hashes(o.data) for o in objects]
        self.assertNotIn((original["vertices_sha256"], original["triangles_sha256"]), hashes)

    def test_mesh_off_brings_no_models(self):
        _, objects = run(VALID / "cube.bread", import_mesh=False)
        self.assertEqual(objects, [])

    def test_support_previews_show_when_there_is_nothing_to_rebuild(self):
        _, objects = run(VALID / "support-preview.bread")
        self.assertTrue([o for o in objects if o.name.endswith("supports")])
        _, objects = run(VALID / "support-preview.bread", import_supports=False)
        self.assertFalse([o for o in objects if o.name.endswith("supports")])

    def test_the_dialog_says_what_the_file_holds(self):
        from bread_import import importer

        lines, flags = importer._summary_text(str(VALID / "slicer-data.bread"))
        self.assertEqual(flags, (True, True))
        self.assertEqual(lines[0], "1 model")
        _, flags = importer._summary_text(str(VALID / "cube.bread"))
        self.assertEqual(flags, (False, False))
        lines, _ = importer._summary_text(str(REPO / "conformance" / "invalid" / "cut-short.bread"))
        self.assertTrue(lines[0].startswith("Cannot read it"))

    def test_a_refused_file_imports_nothing(self):
        clear()
        with self.assertRaises(RuntimeError):
            bpy.ops.import_scene.bread(filepath=str(REPO / "conformance" / "invalid" / "crc-mismatch.bread"))
        self.assertEqual(len(bpy.data.objects), 0)


if __name__ == "__main__":
    bread_import.register()
    try:
        program = unittest.main(argv=[sys.argv[0]], exit=False, verbosity=2)
    finally:
        bread_import.unregister()
    sys.exit(0 if program.result.wasSuccessful() else 1)
