# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""The import operator, the File > Import entry, and dropping .bread files
on the 3D Viewport or the Outliner."""

from __future__ import annotations

import math
import os
import time

import bpy
import numpy as np
from bpy.props import BoolProperty, CollectionProperty, EnumProperty, FloatProperty, StringProperty
from bpy_extras.io_utils import ImportHelper, poll_file_object_drop
from mathutils import Matrix

from . import contents as bread
from . import supports
from .native import BreadError

# The colour of supports in the viewport: the tone of bread.synrgs.tech.
SUPPORT_COLOUR = (0.68, 0.39, 0.10, 1.0)


def _summary_text(path: str) -> tuple[list[str], tuple[bool, bool]]:
    """What the file holds, for the import dialog, kept until it changes:
    lines to show, and whether it has hollows and supports."""
    try:
        stamp = os.stat(path).st_mtime_ns
    except OSError:
        return [], (True, True)
    cached = _summary_text.cache.get(path)
    if cached and cached[0] == stamp:
        return cached[1]
    try:
        with open(path, "rb") as f:
            c = bread.read(f.read())
        models = len(c.models())
        lines = [
            f"{models} model{'s' if models != 1 else ''}",
            f"{len(c.hollows)} hollowed" if c.hollows else "nothing hollowed",
            f"{len(c.supports)} set{'s' if len(c.supports) != 1 else ''} of supports"
            if c.supports else "no supports",
        ]
        flags = (bool(c.hollows), bool(c.supports or any(ch.tag == "SPRV" for ch in c.chunks)))
    except (OSError, BreadError) as e:
        lines, flags = [f"Cannot read it: {e}"], (False, False)
    _summary_text.cache[path] = (stamp, (lines, flags))
    return lines, flags


_summary_text.cache = {}


def _new_mesh(name: str, vertices: np.ndarray, triangles: np.ndarray) -> bpy.types.Mesh:
    """A Blender mesh with these vertices, bit for bit, and these triangles,
    in their order and with their first corners."""
    me = bpy.data.meshes.new(name)
    n = len(triangles)
    me.vertices.add(len(vertices))
    me.vertices.foreach_set("co", np.ascontiguousarray(vertices, dtype=np.float32).ravel())
    me.loops.add(3 * n)
    me.loops.foreach_set("vertex_index", np.ascontiguousarray(triangles, dtype=np.int32).ravel())
    me.polygons.add(n)
    me.polygons.foreach_set("loop_start", np.arange(0, 3 * n, 3, dtype=np.int32))
    me.update(calc_edges=True)
    return me


def _support_material() -> bpy.types.Material:
    mat = bpy.data.materials.get("bread supports")
    if mat is None:
        mat = bpy.data.materials.new("bread supports")
        mat.diffuse_color = SUPPORT_COLOUR
        mat.roughness = 0.6
    return mat


class IMPORT_SCENE_OT_bread(bpy.types.Operator, ImportHelper):
    """Import a .bread file: its models, hollows and supports"""

    bl_idname = "import_scene.bread"
    bl_label = "Import .bread"
    bl_options = {"REGISTER", "UNDO", "PRESET"}

    filename_ext = ".bread"
    filter_glob: StringProperty(default="*.bread", options={"HIDDEN"})
    files: CollectionProperty(type=bpy.types.OperatorFileListElement, options={"HIDDEN", "SKIP_SAVE"})
    directory: StringProperty(subtype="DIR_PATH", options={"HIDDEN", "SKIP_SAVE"})

    import_mesh: BoolProperty(
        name="Mesh",
        description="The models, as the file shows them",
        default=True,
    )
    import_hollow: BoolProperty(
        name="Hollow",
        description="Hollowed models come hollow, as they print. "
        "Off, they come solid: the original the file keeps",
        default=True,
    )
    import_supports: BoolProperty(
        name="Supports",
        description="The supports, rebuilt from the sizes and places the file keeps",
        default=True,
    )
    support_detail: EnumProperty(
        name="Detail",
        description="How round the supports are",
        items=(
            ("LOW", "Low", "8 sides: light, for many supports"),
            ("MEDIUM", "Medium", "12 sides"),
            ("HIGH", "High", "24 sides: smooth, for close-ups and renders"),
        ),
        default="MEDIUM",
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

    def invoke(self, context, event):
        return self.invoke_popup(context)

    def _paths(self) -> list[str]:
        if self.files and self.directory:
            return [os.path.join(self.directory, f.name) for f in self.files if f.name]
        return [self.filepath] if self.filepath else []

    def draw(self, context):
        layout = self.layout
        layout.use_property_split = False
        paths = self._paths()
        has_hollow, has_supports = True, True
        if len(paths) == 1 and os.path.isfile(paths[0]):
            lines, (has_hollow, has_supports) = _summary_text(paths[0])
            box = layout.box()
            col = box.column(align=True)
            for line in lines:
                col.label(text=line)

        col = layout.column(heading="Import")
        col.prop(self, "import_mesh")
        row = col.row()
        row.active = self.import_mesh and has_hollow
        row.prop(self, "import_hollow")
        row = col.row()
        row.active = has_supports
        row.prop(self, "import_supports")
        row = col.row()
        row.active = self.import_supports and has_supports
        row.prop(self, "support_detail")
        layout.use_property_split = True
        layout.prop(self, "global_scale")

    def execute(self, context):
        paths = self._paths()
        if not paths:
            self.report({"ERROR"}, "No file to import")
            return {"CANCELLED"}
        imported = []
        for path in paths:
            try:
                imported += self._import(context, path)
            except (OSError, BreadError) as e:
                self.report({"ERROR"}, f"{os.path.basename(path)}: cannot read it ({e})")
        if not imported:
            return {"CANCELLED"}
        for obj in context.view_layer.objects.selected:
            obj.select_set(False)
        for obj in imported:
            obj.select_set(True)
        context.view_layer.objects.active = imported[0]
        return {"FINISHED"}

    def _import(self, context, path: str) -> list[bpy.types.Object]:
        started = time.perf_counter()
        with open(path, "rb") as f:
            c = bread.read(f.read())
        stem = os.path.splitext(os.path.basename(path))[0]
        collection = bpy.data.collections.new(stem)
        context.view_layer.active_layer_collection.collection.children.link(collection)
        scale = Matrix.Scale(self.global_scale, 4)
        objects = []
        meshes: dict[int, bpy.types.Mesh] = {}
        names = {}

        def leaf_name(leaf: int, mesh: int | None) -> str:
            node = c.scene.nodes[leaf]
            if node.name:
                return node.name
            if mesh is not None and c.mesh(mesh).name:
                return c.mesh(mesh).name
            return stem

        def add(name, data, matrix):
            obj = bpy.data.objects.new(name, data)
            obj.matrix_world = scale @ Matrix(matrix.tolist())
            collection.objects.link(obj)
            objects.append(obj)
            return obj

        if self.import_mesh:
            hollows = c.hollow_models
            for p in c.models():
                index = p.mesh
                if p.leaf in hollows and not self.import_hollow:
                    index = hollows[p.leaf]
                m = c.mesh(index)
                if len(m.triangles) == 0:
                    continue
                name = leaf_name(p.leaf, index)
                names[p.leaf] = name
                if index not in meshes:
                    meshes[index] = _new_mesh(name, m.vertices, m.triangles)
                add(name, meshes[index], p.matrix)

        built = plate_rafts = 0
        if self.import_supports:
            by_leaf = {}
            for p in c.placements:
                by_leaf.setdefault(p.leaf, []).append(p)
            previews = None
            done = set()
            for entry in c.supports:
                model_name = names.get(entry.model) or leaf_name(entry.model, c.scene.leaf_mesh(entry.model))
                data = None
                try:
                    doc = supports.read_document(entry.generator, entry.version, entry.document)
                    v, t = supports.build(doc, self.support_detail)
                    if len(t):
                        # The document is in the scene's space: each copy of
                        # the support leaf gets it in the leaf's own space.
                        data = (v, t, True)
                except supports.SupportsError as e:
                    if previews is None:
                        previews = c.previews()
                    preview = previews.get(entry.part)
                    if preview is not None:
                        data = (preview.vertices, preview.triangles, False)
                    else:
                        self.report({"WARNING"}, f"{stem}: supports of {model_name} not rebuilt: {e}")
                if data is None:
                    continue
                v, t, in_scene_space = data
                for p in by_leaf.get(entry.part, []):
                    verts = v
                    if in_scene_space and not np.allclose(p.matrix, np.identity(4)):
                        inv = np.linalg.inv(p.matrix)
                        verts = (v.astype(np.float64) @ inv[:3, :3].T + inv[:3, 3]).astype(np.float32)
                    me = _new_mesh(f"{model_name} supports", verts, t)
                    me.shade_smooth()
                    if hasattr(me, "set_sharp_from_angle"):
                        me.set_sharp_from_angle(angle=math.radians(40))
                    me.materials.append(_support_material())
                    add(f"{model_name} supports", me, p.matrix)
                    built += 1
                done.add(entry.part)
            # Files that keep only a preview of their supports.
            if any(ch.tag == "SPRV" for ch in c.chunks):
                if previews is None:
                    previews = c.previews()
                for leaf, preview in previews.items():
                    if leaf in done:
                        continue
                    me = _new_mesh(f"{stem} supports", preview.vertices, preview.triangles)
                    me.materials.append(_support_material())
                    for p in by_leaf.get(leaf, []):
                        add(f"{stem} supports", me, p.matrix)
                        built += 1
            # Breadslicer's one raft for every model, kept as is.
            rafts = {}
            for p in c.plate_rafts():
                if p.mesh not in rafts:
                    m = c.mesh(p.mesh)
                    rafts[p.mesh] = _new_mesh(bread.PLATE_RAFT_NAME, m.vertices, m.triangles)
                    rafts[p.mesh].materials.append(_support_material())
                add(bread.PLATE_RAFT_NAME, rafts[p.mesh], p.matrix)
                plate_rafts += 1

        models = len(objects) - built - plate_rafts
        raft = f", {plate_rafts} raft for all" if plate_rafts else ""
        self.report(
            {"INFO"},
            f"{stem}: {models} model{'s' if models != 1 else ''}, "
            f"{built} set{'s' if built != 1 else ''} of supports{raft} "
            f"in {time.perf_counter() - started:.2f} s",
        )
        return objects


class IO_FH_bread(bpy.types.FileHandler):
    bl_idname = "IO_FH_bread"
    bl_label = ".bread"
    bl_import_operator = "import_scene.bread"
    bl_file_extensions = ".bread"

    @classmethod
    def poll_drop(cls, context):
        return poll_file_object_drop(context)


def menu_import(self, context):
    self.layout.operator(IMPORT_SCENE_OT_bread.bl_idname, text="Bread (.bread)")


classes = (IMPORT_SCENE_OT_bread, IO_FH_bread)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.TOPBAR_MT_file_import.append(menu_import)


def unregister():
    bpy.types.TOPBAR_MT_file_import.remove(menu_import)
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
