# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""The export operator and the File > Export entry: the objects of the
scene, or the selected ones, each a named part of one .bread file."""

from __future__ import annotations

import os
import time
from datetime import datetime, timezone

import bpy
import numpy as np
from bpy.props import BoolProperty, FloatProperty, StringProperty
from bpy_extras.io_utils import ExportHelper

from . import writing
from .native import BreadError

# Objects Blender can turn into a mesh.
MESHABLE = {"MESH", "CURVE", "SURFACE", "FONT", "META"}


def _version() -> str:
    """The extension's version, from its manifest."""
    manifest = os.path.join(os.path.dirname(__file__), "blender_manifest.toml")
    try:
        with open(manifest, encoding="utf-8") as f:
            for line in f:
                if line.startswith("version"):
                    return line.split("=", 1)[1].strip().strip('"')
    except OSError:
        pass
    return "?"


def _mesh_data(me: bpy.types.Mesh) -> writing.MeshData:
    """A Blender mesh's vertices, bit for bit, and its faces as triangles. A
    mesh of triangles keeps their order and first corners, so a .bread file
    imported and exported again comes back the same."""
    vertices = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get("co", vertices)
    n = len(me.polygons)
    totals = np.empty(n, dtype=np.int32)
    me.polygons.foreach_get("loop_total", totals)
    if n and np.all(totals == 3):
        starts = np.empty(n, dtype=np.int32)
        me.polygons.foreach_get("loop_start", starts)
        corners = np.empty(len(me.loops), dtype=np.int32)
        me.loops.foreach_get("vertex_index", corners)
        triangles = corners[starts[:, None] + np.arange(3)]
    else:
        me.calc_loop_triangles()
        triangles = np.empty(len(me.loop_triangles) * 3, dtype=np.int32)
        me.loop_triangles.foreach_get("vertices", triangles)
    return writing.MeshData(vertices.reshape(-1, 3), triangles.reshape(-1, 3).astype(np.uint32))


class EXPORT_SCENE_OT_bread(bpy.types.Operator, ExportHelper):
    """Export the scene, or the selected objects, as a .bread file: each object a named part"""

    bl_idname = "export_scene.bread"
    bl_label = "Export .bread"
    bl_options = {"PRESET"}

    filename_ext = ".bread"
    filter_glob: StringProperty(default="*.bread", options={"HIDDEN"})

    use_selection: BoolProperty(
        name="Selection Only",
        description="Export the selected objects only",
        default=False,
    )
    use_mesh_modifiers: BoolProperty(
        name="Apply Modifiers",
        description="Export each object as it shows, with its modifiers applied. "
        "An object without modifiers is exported bit for bit either way",
        default=True,
    )
    global_scale: FloatProperty(
        name="Scale",
        description="Blender units per millimetre: lengths in a .bread file are millimetres",
        default=1.0,
        min=1e-6,
        max=1e6,
        soft_min=0.001,
        soft_max=1000.0,
    )
    title: StringProperty(
        name="Title",
        description="The title of the model. Empty: the file has none",
    )
    designer: StringProperty(
        name="Designer",
        description="Who designed it: it travels with every copy of the file",
    )
    license: StringProperty(
        name="License",
        description="Under what terms it may be used, as an SPDX license expression, "
        "such as CC-BY-4.0 or CC-BY-NC-4.0",
    )
    source: StringProperty(
        name="Source",
        description="Where it comes from, as a link: the designer's page or shop",
    )

    def invoke(self, context, event):
        if not self.title:
            name = bpy.path.display_name_from_filepath(bpy.data.filepath)
            self.title = name or ""
        return super().invoke(context, event)

    def draw(self, context):
        layout = self.layout
        layout.use_property_split = True
        layout.use_property_decorate = False
        col = layout.column(heading="Include")
        col.prop(self, "use_selection")
        col.prop(self, "use_mesh_modifiers")
        layout.prop(self, "global_scale")
        box = layout.box()
        box.label(text="Credit, saved in the file")
        for name in ("title", "designer", "license", "source"):
            box.prop(self, name)

    def _objects(self, context) -> list[bpy.types.Object]:
        pool = context.selected_objects if self.use_selection else context.view_layer.objects
        kinds = MESHABLE if self.use_mesh_modifiers else {"MESH"}
        return sorted((o for o in pool if o.type in kinds), key=lambda o: o.name)

    def execute(self, context):
        started = time.perf_counter()
        objects = self._objects(context)
        if not objects:
            self.report({"ERROR"}, "Nothing to export: no objects with a mesh")
            return {"CANCELLED"}
        depsgraph = context.evaluated_depsgraph_get()
        to_mm = np.diag([1.0 / self.global_scale] * 3 + [1.0])
        meshes: list[writing.MeshData] = []
        parts: list[writing.Part] = []
        shared: dict[str, int] = {}
        skipped = []
        for obj in objects:
            # A mesh without modifiers or shape keys is its own data, bit for
            # bit, and stored once however many objects show it.
            plain = obj.type == "MESH" and (
                not self.use_mesh_modifiers or (not obj.modifiers and not obj.data.shape_keys)
            )
            if plain and obj.data.name_full in shared:
                index = shared[obj.data.name_full]
            else:
                if plain:
                    data = _mesh_data(obj.data)
                else:
                    evaluated = obj.evaluated_get(depsgraph)
                    try:
                        data = _mesh_data(evaluated.to_mesh())
                    finally:
                        evaluated.to_mesh_clear()
                if len(data.triangles) == 0:
                    skipped.append(obj.name)
                    continue
                index = len(meshes)
                meshes.append(data)
                if plain:
                    shared[obj.data.name_full] = index
            world = np.array(obj.matrix_world, dtype=np.float64)
            matrix = to_mm @ world
            parts.append(writing.Part(obj.name, index, None if np.array_equal(matrix, np.identity(4)) else matrix))
        if not parts:
            self.report({"ERROR"}, "Nothing to export: the objects have no faces")
            return {"CANCELLED"}

        metadata = [
            (None, key, value)
            for key, value in (
                ("title", self.title.strip()),
                ("designer", self.designer.strip()),
                ("license", self.license.strip()),
                ("source", self.source.strip()),
                ("created", datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")),
                ("generator", f"Bread (.bread) for Blender {_version()}, Blender {bpy.app.version_string}"),
            )
            if value
        ]
        try:
            data = writing.write(meshes, parts, metadata)
            with open(self.filepath, "wb") as f:
                f.write(data)
        except (OSError, BreadError) as e:
            self.report({"ERROR"}, f"Cannot write {os.path.basename(self.filepath)}: {e}")
            return {"CANCELLED"}
        if skipped:
            self.report({"WARNING"}, f"Left out, no faces: {', '.join(skipped)}")
        self.report(
            {"INFO"},
            f"{os.path.basename(self.filepath)}: {len(parts)} part{'s' if len(parts) != 1 else ''}, "
            f"{len(data) / 1e6:.2f} MB in {time.perf_counter() - started:.2f} s",
        )
        return {"FINISHED"}


def menu_export(self, context):
    self.layout.operator(EXPORT_SCENE_OT_bread.bl_idname, text="Bread (.bread)")


def register():
    bpy.utils.register_class(EXPORT_SCENE_OT_bread)
    bpy.types.TOPBAR_MT_file_export.append(menu_export)


def unregister():
    bpy.types.TOPBAR_MT_file_export.remove(menu_export)
    bpy.utils.unregister_class(EXPORT_SCENE_OT_bread)
