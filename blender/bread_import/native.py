# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Reads .bread files with the bread-format crate, built as a native library
from crates/bread-wasm: the same code, and the same checks, as every other
reader of the format. Its C ABI is the one the JavaScript package calls;
here ctypes calls it."""

from __future__ import annotations

import ctypes
import os
import sys
from dataclasses import dataclass, field

import numpy as np

_LIBRARY_NAMES = {
    "win32": "bread.dll",
    "linux": "libbread.so",
    "darwin": "libbread.dylib",
}

NO_TEXT = ctypes.c_size_t(-1).value
SCENE_ITSELF = NO_TEXT
NODE_KINDS = {0: "group", 1: "leaf", 2: "shared leaf", 4: "reference"}


class BreadError(Exception):
    """A file the reader refused. ``reason`` is a reason code of the
    conformance suite, such as "crc"; ``chunk`` the chunk at fault, or None."""

    def __init__(self, reason: str, chunk: str | None = None):
        self.reason = reason
        self.chunk = chunk
        super().__init__(f"{reason} ({chunk})" if chunk else reason)


def _tag(value: int) -> str:
    return value.to_bytes(4, "little").decode("ascii", "replace")


def _load() -> ctypes.CDLL:
    name = _LIBRARY_NAMES.get(sys.platform)
    if name is None:
        raise OSError(f"no .bread library for {sys.platform}")
    lib = ctypes.CDLL(os.path.join(os.path.dirname(__file__), "lib", name))
    size, u32, f64p = ctypes.c_size_t, ctypes.c_uint32, ctypes.POINTER(ctypes.c_double)
    ptr = ctypes.c_void_p
    signatures = {
        "bread_read_chunks": (u32, [ctypes.c_char_p, size]),
        "bread_decode_mesh": (u32, [u32, ptr, size]),
        "bread_decode_scene": (u32, [u32, size, ptr, size]),
        "bread_decode_previews": (u32, [u32, ptr, size, u32, ptr, size, ptr, size]),
        "bread_preview_count": (size, []),
        "bread_preview_leaf": (size, [size]),
        "bread_preview_select": (u32, [size]),
        "bread_release": (None, []),
        "bread_chunk_count": (size, []),
        "bread_chunk_tag": (u32, [size]),
        "bread_chunk_version": (u32, [size]),
        "bread_chunk_payload": (ptr, [size]),
        "bread_chunk_payload_len": (size, [size]),
        "bread_mesh_vertex_count": (size, []),
        "bread_mesh_vertices": (ptr, []),
        "bread_mesh_triangle_count": (size, []),
        "bread_mesh_triangles": (ptr, []),
        "bread_mesh_has_meta": (u32, []),
        "bread_mesh_text": (ptr, [u32]),
        "bread_mesh_text_len": (size, [u32]),
        "bread_scene_has_meta": (u32, [size]),
        "bread_scene_text": (ptr, [size, u32]),
        "bread_scene_text_len": (size, [size, u32]),
        "bread_scene_node_count": (size, []),
        "bread_scene_node_kind": (u32, [size]),
        "bread_scene_node_target": (size, [size]),
        "bread_scene_node_children": (ptr, [size]),
        "bread_scene_node_children_len": (size, [size]),
        "bread_scene_node_transform": (f64p, [size]),
        "bread_scene_list": (ptr, [u32]),
        "bread_scene_list_len": (size, [u32]),
        "bread_refusal_reason": (ptr, []),
        "bread_refusal_reason_len": (size, []),
        "bread_refusal_chunk": (u32, []),
    }
    for fn, (restype, argtypes) in signatures.items():
        f = getattr(lib, fn)
        f.restype = restype
        f.argtypes = argtypes
    return lib


_lib: ctypes.CDLL | None = None


def library() -> ctypes.CDLL:
    global _lib
    if _lib is None:
        _lib = _load()
    return _lib


@dataclass
class Chunk:
    tag: str
    version: int
    payload: bytes


@dataclass
class Node:
    kind: str
    name: str | None
    # Twelve f64: the linear part row by row, then the translation; None for
    # the identity.
    transform: tuple[float, ...] | None
    children: list[int] = field(default_factory=list)
    mesh: int | None = None  # a leaf's mesh
    resource: int | None = None  # a shared leaf's mesh resource
    subtree: int | None = None  # a reference's subtree


@dataclass
class Scene:
    name: str | None
    nodes: list[Node]
    mesh_resources: list[int]
    subtrees: list[int]
    roots: list[int]

    def leaf_mesh(self, index: int) -> int | None:
        node = self.nodes[index]
        if node.kind == "leaf":
            return node.mesh
        if node.kind == "shared leaf":
            return self.mesh_resources[node.resource]
        return None


@dataclass
class Mesh:
    name: str | None
    vertices: np.ndarray  # (n, 3) float32, bit for bit as stored
    triangles: np.ndarray  # (m, 3) uint32, in file order, first corner first


def _refusal(lib: ctypes.CDLL) -> BreadError:
    reason = ctypes.string_at(lib.bread_refusal_reason(), lib.bread_refusal_reason_len())
    chunk = lib.bread_refusal_chunk()
    return BreadError(reason.decode("utf-8", "replace"), _tag(chunk) if chunk else None)


def _indices(pointer: int | None, length: int) -> list[int]:
    if not length:
        return []
    return list((ctypes.c_size_t * length).from_address(pointer))


def read_chunks(data: bytes) -> list[Chunk]:
    """The chunks of a file this reader knows, checked and decompressed, in
    file order. Raises BreadError when the file is refused."""
    lib = library()
    try:
        if not lib.bread_read_chunks(data, len(data)):
            raise _refusal(lib)
        chunks = []
        for i in range(lib.bread_chunk_count()):
            payload = ctypes.string_at(lib.bread_chunk_payload(i), lib.bread_chunk_payload_len(i))
            chunks.append(Chunk(_tag(lib.bread_chunk_tag(i)), lib.bread_chunk_version(i), payload))
        return chunks
    finally:
        lib.bread_release()


def _mesh_text(lib: ctypes.CDLL, which: int) -> str | None:
    length = lib.bread_mesh_text_len(which)
    if length == NO_TEXT:
        return None
    return ctypes.string_at(lib.bread_mesh_text(which), length).decode("utf-8", "replace")


def _current_mesh(lib: ctypes.CDLL) -> Mesh:
    nv = lib.bread_mesh_vertex_count()
    nt = lib.bread_mesh_triangle_count()
    vertices = np.empty((nv, 3), dtype=np.float32)
    triangles = np.empty((nt, 3), dtype=np.uint32)
    if nv:
        ctypes.memmove(vertices.ctypes.data, lib.bread_mesh_vertices(), vertices.nbytes)
    if nt:
        ctypes.memmove(triangles.ctypes.data, lib.bread_mesh_triangles(), triangles.nbytes)
    name = _mesh_text(lib, 0) if lib.bread_mesh_has_meta() else None
    return Mesh(name, vertices, triangles)


def decode_mesh(chunk: Chunk) -> Mesh:
    lib = library()
    buffer = ctypes.create_string_buffer(chunk.payload, len(chunk.payload))
    try:
        if not lib.bread_decode_mesh(chunk.version, buffer, len(chunk.payload)):
            raise _refusal(lib)
        return _current_mesh(lib)
    finally:
        lib.bread_release()


def decode_scene(chunks: list[Chunk]) -> Scene:
    scenes = [c for c in chunks if c.tag == "SCNE"]
    if len(scenes) != 1:
        raise BreadError("chunk-count", "SCNE")
    mesh_count = sum(1 for c in chunks if c.tag == "MESH")
    lib = library()
    payload = scenes[0].payload
    buffer = ctypes.create_string_buffer(payload, len(payload))
    try:
        if not lib.bread_decode_scene(scenes[0].version, mesh_count, buffer, len(payload)):
            raise _refusal(lib)

        def name(target: int) -> str | None:
            if not lib.bread_scene_has_meta(target):
                return None
            length = lib.bread_scene_text_len(target, 0)
            if length == NO_TEXT:
                return None
            return ctypes.string_at(lib.bread_scene_text(target, 0), length).decode("utf-8", "replace")

        nodes = []
        for i in range(lib.bread_scene_node_count()):
            at = lib.bread_scene_node_transform(i)
            transform = tuple(at[k] for k in range(12)) if at else None
            node = Node(NODE_KINDS.get(lib.bread_scene_node_kind(i), "?"), name(i), transform)
            target = lib.bread_scene_node_target(i)
            if node.kind == "group":
                node.children = _indices(
                    lib.bread_scene_node_children(i), lib.bread_scene_node_children_len(i)
                )
            elif node.kind == "leaf":
                node.mesh = target
            elif node.kind == "shared leaf":
                node.resource = target
            else:
                node.subtree = target
            nodes.append(node)
        lists = [_indices(lib.bread_scene_list(w), lib.bread_scene_list_len(w)) for w in range(3)]
        return Scene(name(SCENE_ITSELF), nodes, lists[0], lists[1], lists[2])
    finally:
        lib.bread_release()


def decode_previews(chunks: list[Chunk], empty: list[bool]) -> dict[int, Mesh]:
    """The support previews of the file (SPEC 9), by support leaf. ``empty``
    says, for each mesh chunk, whether it has no vertices and no triangles."""
    previews = [c for c in chunks if c.tag == "SPRV"]
    if not previews:
        return {}
    scene = next(c for c in chunks if c.tag == "SCNE")
    lib = library()
    p = ctypes.create_string_buffer(previews[0].payload, len(previews[0].payload))
    s = ctypes.create_string_buffer(scene.payload, len(scene.payload))
    flags = bytes(1 if e else 0 for e in empty)
    e = ctypes.create_string_buffer(flags, len(flags))
    try:
        if not lib.bread_decode_previews(
            previews[0].version, p, len(previews[0].payload),
            scene.version, s, len(scene.payload), e, len(flags),
        ):
            raise _refusal(lib)
        out = {}
        for i in range(lib.bread_preview_count()):
            leaf = lib.bread_preview_leaf(i)
            if lib.bread_preview_select(i):
                out[leaf] = _current_mesh(lib)
        return out
    finally:
        lib.bread_release()
