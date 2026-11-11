# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The reader against every conformance file (conformance/README.md)."""

from __future__ import annotations

import hashlib
import json
import unittest
from pathlib import Path

from bread_format import BreadError, Mesh, read_file

CONFORMANCE = Path(__file__).resolve().parents[2] / "conformance"


def cases(folder: str):
    for path in sorted((CONFORMANCE / folder).glob("*.bread")):
        yield path.stem, path.read_bytes(), json.loads(path.with_suffix(".json").read_text("utf-8"))


def meta(m):
    return None if m is None else {"name": m.name, "comment": m.comment}


def mesh_json(mesh: Mesh):
    return {
        "meta": meta(mesh.meta),
        "vertex_count": mesh.vertex_count,
        "triangle_count": mesh.triangle_count,
        "vertices_sha256": hashlib.sha256(mesh.vertex_bytes()).hexdigest(),
        "triangles_sha256": hashlib.sha256(mesh.triangle_bytes()).hexdigest(),
    }


def node_json(node):
    out = {"kind": node.kind, "meta": meta(node.meta)}
    t = node.transform
    out["transform"] = None if t is None else {"linear": [list(r) for r in t[:3]], "translation": list(t[3])}
    if node.kind == "group":
        out["children"] = list(node.children)
    elif node.kind == "leaf":
        out["mesh"] = node.mesh
    elif node.kind == "shared leaf":
        out["resource"] = node.resource
    else:
        out["subtree"] = node.subtree
    return out


def blob(data):
    return None if data is None else {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def document(entry):
    return {
        "generator": entry.generator,
        "version": entry.version,
        "document_size": len(entry.document),
        "document_sha256": hashlib.sha256(entry.document).hexdigest(),
    }


def file_json(bread) -> dict:
    scene = bread.scene
    return {
        "meta": meta(scene.meta),
        "meshes": [mesh_json(m) for m in bread.meshes],
        "mesh_resources": list(scene.mesh_resources),
        "nodes": [node_json(n) for n in scene.nodes],
        "subtrees": list(scene.subtrees),
        "roots": list(scene.roots),
        "settings": blob(bread.settings),
        "supports": [{"model": e.model, "support": e.part, **document(e)} for e in bread.supports],
        "hollows": [{"model": e.model, "original": e.part, **document(e)} for e in bread.hollows],
        "thumbnail": blob(bread.thumbnail),
        "support_previews": [{"leaf": leaf, "mesh": mesh_json(m)} for leaf, m in bread.support_previews.items()],
        "surface": [
            {
                "leaf": p.leaf,
                "kind": p.kind,
                "version": p.version,
                "document_size": len(p.document),
                "document_sha256": hashlib.sha256(p.document).hexdigest(),
            }
            for p in bread.surface
        ],
    }


class Valid(unittest.TestCase):
    def test_every_valid_file_reads_as_its_json_says(self):
        count = 0
        for name, data, want in cases("valid"):
            with self.subTest(name):
                got = file_json(read_file(data))
                want = {k: v for k, v in want.items() if k != "description"}
                self.assertEqual(got, want)
            count += 1
        self.assertTrue(count, "no valid files found")


class Invalid(unittest.TestCase):
    def test_every_invalid_file_is_refused_for_its_reason(self):
        count = 0
        for name, data, want in cases("invalid"):
            with self.subTest(name):
                with self.assertRaises(BreadError) as caught:
                    read_file(data)
                self.assertEqual((caught.exception.reason, caught.exception.chunk), (want["reason"], want["chunk"]))
            count += 1
        self.assertTrue(count, "no invalid files found")


if __name__ == "__main__":
    unittest.main()
