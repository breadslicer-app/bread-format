# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""A whole file: the container, then every chunk this reader knows, checked
against each other."""

from __future__ import annotations

from dataclasses import dataclass

from .container import RawChunk, decompress, split_chunks
from .errors import BreadError, Malformed, NoSuchVertex
from .mesh import Mesh, read_mesh
from .scene import Scene, read_scene
from .slicer import Entry, Paint, read_entries, read_paint, read_previews

#: The chunk versions this reader reads, by tag (SPEC 7.1).
VERSIONS = {
    b"SCNE": (1,),
    b"MESH": (1, 2),
    b"THMB": (1,),
    b"PROJ": (1,),
    b"SUPP": (1, 2),
    b"HOLW": (1, 2),
    b"SPRV": (1,),
    b"SURF": (1,),
}

#: How many chunks of each tag a file holds: (at least, at most).
PER_FILE = {b"SCNE": (1, 1), b"MESH": (0, None)}


@dataclass(frozen=True)
class BreadFile:
    """What a file holds.

    ``settings`` is the PROJ payload, a TOML document, and ``thumbnail`` the
    THMB payload, a PNG; each is None when the file has none. ``supports``
    and ``hollows`` are the SUPP and HOLW entries, ``support_previews`` maps
    each support leaf with a preview to its mesh, and ``surface`` holds the
    SURF entries."""

    scene: Scene
    meshes: list[Mesh]
    settings: bytes | None
    thumbnail: bytes | None
    supports: list[Entry]
    hollows: list[Entry]
    support_previews: dict[int, Mesh]
    surface: list[Paint]

    def placements(self):
        """Every mesh shown, with its world transform (SPEC 5.4)."""
        for leaf, mesh, transform in self.scene.placements():
            yield leaf, self.meshes[mesh], transform


def read_file(data: bytes) -> BreadFile:
    """Reads a whole ``.bread`` file.

    Raises BreadError, with its reason code, when the file is refused."""
    chunks: dict[bytes, list[RawChunk]] = {}
    for chunk in split_chunks(data):
        versions = VERSIONS.get(chunk.tag)
        if versions is None:
            continue  # a tag this reader does not know: skipped (SPEC 3.4)
        if chunk.version > max(versions) or chunk.version < 1:
            raise BreadError("version", chunk.name)
        chunks.setdefault(chunk.tag, []).append(chunk)

    for tag in VERSIONS:
        least, most = PER_FILE.get(tag, (0, 1))
        found = len(chunks.get(tag, []))
        if found < least or (most is not None and found > most):
            raise BreadError("chunk-count", tag.decode())

    def payload(chunk: RawChunk) -> bytes:
        return decompress(chunk)

    def one(tag: bytes) -> RawChunk | None:
        found = chunks.get(tag)
        return found[0] if found else None

    (scene_chunk,) = chunks[b"SCNE"]
    scene = _guard("SCNE", read_scene, payload(scene_chunk))

    meshes = [_guard("MESH", read_mesh, chunk.version, payload(chunk)) for chunk in chunks.get(b"MESH", [])]
    if scene.mesh_count != len(meshes):
        raise BreadError("malformed", "SCNE")

    settings = payload(c) if (c := one(b"PROJ")) else None
    thumbnail = payload(c) if (c := one(b"THMB")) else None

    # SUPP and HOLW of version 1 were drafts: a reader may skip them (SPEC 7.3).
    supports = _envelope("SUPP", payload(c)) if (c := one(b"SUPP")) and c.version == 2 else []
    hollows = _envelope("HOLW", payload(c)) if (c := one(b"HOLW")) and c.version == 2 else []
    _check_entries(scene, len(meshes), supports, hollows)

    previews: dict[int, Mesh] = {}
    if c := one(b"SPRV"):
        support_leaves = {entry.part for entry in supports} if one(b"SUPP") else None
        for preview in _guard("SPRV", read_previews, payload(c)):
            mesh = _guard("SPRV", _preview_mesh, preview.mesh)
            leaf = preview.leaf
            if not _is_leaf(scene, leaf) or leaf in previews:
                raise BreadError("malformed", "SPRV")
            shown = meshes[scene.mesh_of(leaf)]
            if shown.vertex_count or shown.triangle_count:
                raise BreadError("malformed", "SPRV")
            if support_leaves is not None and leaf not in support_leaves:
                raise BreadError("malformed", "SPRV")
            previews[leaf] = mesh

    surface: list[Paint] = []
    if c := one(b"SURF"):
        surface = _guard("SURF", read_paint, payload(c))
        seen = set()
        for paint in surface:
            if not _is_leaf(scene, paint.leaf) or (paint.leaf, paint.kind) in seen:
                raise BreadError("malformed", "SURF")
            seen.add((paint.leaf, paint.kind))

    return BreadFile(scene, meshes, settings, thumbnail, supports, hollows, previews, surface)


def _guard(tag: str, read, *args):
    """Runs a payload reader, turning what it finds into the file's refusal."""
    try:
        return read(*args)
    except Malformed:
        raise BreadError("malformed", tag) from None
    except NoSuchVertex:
        raise BreadError("no-such-vertex", tag) from None


def _preview_mesh(payload: bytes) -> Mesh:
    # A preview's mesh is a MESH payload of version 1, refused as malformed
    # whatever is wrong with it (SPEC 9.2).
    try:
        return read_mesh(1, payload)
    except NoSuchVertex:
        raise Malformed from None


def _envelope(tag: str, payload: bytes) -> list[Entry]:
    return _guard(tag, read_entries, payload)


def _is_leaf(scene: Scene, node: int) -> bool:
    return node < len(scene.nodes) and scene.nodes[node].is_leaf


def _check_entries(scene: Scene, mesh_count: int, supports: list[Entry], hollows: list[Entry]) -> None:
    """SPEC 7.3: what SUPP and HOLW entries name."""
    for tag, entries in (("SUPP", supports), ("HOLW", hollows)):
        models = set()
        for entry in entries:
            if not _is_leaf(scene, entry.model) or entry.model in models:
                raise BreadError("malformed", tag)
            models.add(entry.model)
            if tag == "SUPP" and not _is_leaf(scene, entry.part):
                raise BreadError("malformed", tag)
            if tag == "HOLW" and entry.part >= mesh_count:
                raise BreadError("malformed", tag)
    models = {entry.model for entry in supports} | {entry.model for entry in hollows}
    support_leaves = set()
    for entry in supports:
        if entry.part in support_leaves or entry.part in models:
            raise BreadError("malformed", "SUPP")
        support_leaves.add(entry.part)
