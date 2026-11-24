# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Exports from Blender and reads the files back: every valid conformance
file, imported, exported and imported again, comes back with the same mesh
bits and triangles, in the same places (to within f32: Blender keeps an
object's place as a location, a rotation and a scale); meshes shared stay shared; and the credit given
in the dialog is in the file. Run it with Blender, after blender/build.py:

    blender -b --factory-startup --python-exit-code 1 --python blender/tests/test_export.py
"""

import hashlib
import re
import struct
import sys
import tempfile
import unittest
from pathlib import Path

import bpy
import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE.parent))

import bread_import  # noqa: E402
from bread_import import contents, native  # noqa: E402

VALID = REPO / "conformance" / "valid"
OUT = Path(tempfile.mkdtemp(prefix="bread-export-"))


def clear():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj)
    for me in list(bpy.data.meshes):
        bpy.data.meshes.remove(me)
    for coll in list(bpy.data.collections):
        bpy.data.collections.remove(coll)


def fingerprint(obj):
    """The mesh's vertex bits and triangles, and the object's place."""
    me = obj.data
    co = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get("co", co)
    idx = np.empty(len(me.loops), dtype=np.int32)
    me.loops.foreach_get("vertex_index", idx)
    return (
        hashlib.sha256(co.astype("<f4").tobytes()).hexdigest(),
        hashlib.sha256(idx.astype("<u4").tobytes()).hexdigest(),
        tuple(v for row in obj.matrix_world for v in row),
    )


def models():
    return {o.name: fingerprint(o) for o in bpy.data.objects if o.type == "MESH"}


def metadata(path):
    """The META entries of a file: (subject, key, value)."""
    chunk = next((c for c in native.read_chunks(path.read_bytes()) if c.tag == "META"), None)
    if chunk is None:
        return []
    p, at = chunk.payload, 4
    entries = []
    for _ in range(struct.unpack_from("<I", p)[0]):
        subject, n = struct.unpack_from("<II", p, at)
        key = p[at + 8 : at + 8 + n].decode()
        at += 8 + n
        (m,) = struct.unpack_from("<I", p, at)
        entries.append((subject, key, p[at + 4 : at + 4 + m].decode()))
        at += 4 + m
    return entries


class ExportRoundTrip(unittest.TestCase):
    def test_every_valid_file_comes_back_the_same(self):
        files = sorted(VALID.glob("*.bread"))
        self.assertTrue(files)
        for path in files:
            with self.subTest(path.name):
                clear()
                bpy.ops.import_scene.bread(filepath=str(path), import_supports=False)
                before = models()
                if not before:
                    continue
                out = OUT / path.name
                self.assertEqual(bpy.ops.export_scene.bread(filepath=str(out)), {"FINISHED"})
                # The crate reads it back with every check of a reader.
                contents.read(out.read_bytes())
                clear()
                bpy.ops.import_scene.bread(filepath=str(out), import_supports=False)
                after = models()
                self.assertEqual(after.keys(), before.keys())
                for name, (vertices, triangles, place) in before.items():
                    self.assertEqual(after[name][:2], (vertices, triangles), name)
                    np.testing.assert_allclose(after[name][2], place, rtol=1e-6, atol=1e-6)

    def test_shared_meshes_are_stored_once(self):
        clear()
        bpy.ops.import_scene.bread(filepath=str(VALID / "shared-mesh.bread"))
        objects = [o for o in bpy.data.objects if o.type == "MESH"]
        out = OUT / "shared.bread"
        bpy.ops.export_scene.bread(filepath=str(out))
        c = contents.read(out.read_bytes())
        self.assertEqual(len(c.placements), len(objects))
        self.assertEqual(len(c._mesh_chunks), len({o.data.name for o in objects}))

    def test_the_credit_is_in_the_file(self):
        clear()
        bpy.ops.mesh.primitive_cube_add()
        out = OUT / "credit.bread"
        bpy.ops.export_scene.bread(
            filepath=str(out),
            title="A box",
            designer="A. Designer",
            license="CC-BY-4.0",
            source="https://example.com/box",
        )
        entries = {key: (subject, value) for subject, key, value in metadata(out)}
        whole = 0xFFFFFFFF
        self.assertEqual(entries["title"], (whole, "A box"))
        self.assertEqual(entries["designer"], (whole, "A. Designer"))
        self.assertEqual(entries["license"], (whole, "CC-BY-4.0"))
        self.assertEqual(entries["source"], (whole, "https://example.com/box"))
        self.assertRegex(entries["created"][1], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertTrue(re.match(r"Bread \(\.bread\) for Blender \d", entries["generator"][1]))

    def test_faces_are_triangulated_and_modifiers_applied(self):
        clear()
        bpy.ops.mesh.primitive_cube_add()
        cube = bpy.context.active_object
        out = OUT / "cube.bread"
        bpy.ops.export_scene.bread(filepath=str(out))
        c = contents.read(out.read_bytes())
        self.assertEqual(len(c.mesh(0).triangles), 12)
        cube.modifiers.new("array", "ARRAY").count = 3
        bpy.ops.export_scene.bread(filepath=str(out))
        self.assertEqual(len(contents.read(out.read_bytes()).mesh(0).triangles), 36)
        bpy.ops.export_scene.bread(filepath=str(out), use_mesh_modifiers=False)
        self.assertEqual(len(contents.read(out.read_bytes()).mesh(0).triangles), 12)

    def test_selection_only(self):
        clear()
        bpy.ops.mesh.primitive_cube_add(location=(0, 0, 0))
        bpy.ops.mesh.primitive_cube_add(location=(5, 0, 0))
        picked = bpy.context.active_object
        bpy.ops.object.select_all(action="DESELECT")
        picked.select_set(True)
        out = OUT / "selected.bread"
        bpy.ops.export_scene.bread(filepath=str(out), use_selection=True)
        c = contents.read(out.read_bytes())
        self.assertEqual([c.scene.nodes[p.leaf].name for p in c.placements], [picked.name])
        np.testing.assert_array_equal(c.placements[0].matrix[:3, 3], [5, 0, 0])

    def test_scale_gives_millimetres(self):
        clear()
        bpy.ops.mesh.primitive_cube_add(location=(0.01, 0, 0))
        out = OUT / "metres.bread"
        # One Blender unit is a metre: 0.001 units per millimetre.
        bpy.ops.export_scene.bread(filepath=str(out), global_scale=0.001)
        c = contents.read(out.read_bytes())
        np.testing.assert_allclose(c.placements[0].matrix[:3, 3], [10, 0, 0], rtol=1e-6)
        np.testing.assert_allclose(np.diag(c.placements[0].matrix)[:3], [1000] * 3)


if __name__ == "__main__":
    bread_import.register()
    try:
        program = unittest.main(argv=[sys.argv[0]], exit=False, verbosity=2)
    finally:
        bread_import.unregister()
    sys.exit(0 if program.result.wasSuccessful() else 1)
