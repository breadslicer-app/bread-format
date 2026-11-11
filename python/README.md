# bread-format for Python

Reads `.bread` files: triangle meshes, kept bit for bit, and the scene that places them.

This reader is written from the specification alone, [`SPEC.md`](../SPEC.md), without the code
of the reference implementation, and uses nothing but the Python standard library. It reads every
conformance file the way the reference reader does, which shows that the specification is enough to
read the format. Being plain Python, it is also meant to be read, and to be a starting point for
importing `.bread` into other programs.

It needs Python 3.14 or later, for `compression.zstd`.

## Use

```python
from bread_format import BreadError, read_file

with open("model.bread", "rb") as f:
    bread = read_file(f.read())

for leaf, mesh, transform in bread.placements():
    node = bread.scene.nodes[leaf]
    print(node.meta and node.meta.name, mesh.vertex_count, mesh.triangle_count)
```

`read_file` returns a `BreadFile`:

| Field | What it holds |
|---|---|
| `scene` | the scene: `nodes`, `roots`, `subtrees`, `mesh_resources`, and its `meta` |
| `meshes` | one `Mesh` per `MESH` chunk, in file order |
| `settings` | the `PROJ` payload, a TOML document, or `None` |
| `thumbnail` | the `THMB` payload, a PNG, or `None` |
| `supports`, `hollows` | the `SUPP` and `HOLW` entries |
| `support_previews` | each support leaf with a preview, mapped to its mesh |
| `surface` | the `SURF` entries; `entry.values(mesh)` gives the paint of each triangle |

A `Mesh` keeps its vertices as the bits of their `f32` coordinates, so that every one comes back
exactly, NaN payloads included:

- `mesh.vertex_bytes()` and `mesh.triangle_bytes()` give them as little-endian `f32` and `u32`,
  ready for an array library: `numpy.frombuffer(mesh.vertex_bytes(), "<f4").reshape(-1, 3)`;
- `mesh.positions()` gives each vertex as three Python floats.

`placements()` walks the scene from each root (SPEC 5.4) and gives every leaf shown, its mesh and
its world transform, a 3 × 3 matrix and a translation in `f64`. `apply(transform, point)` places a
point.

Lengths are millimetres, with Z up (SPEC 3.5).

When a file is refused, `read_file` raises `BreadError`, whose `reason` is one of the reason codes
of [`conformance/README.md`](../conformance/README.md), such as `"crc"` or `"malformed"`, and
whose `chunk` names the chunk at fault.

## Speed

The reader favours being easy to follow over being fast: a mesh of 2 million triangles takes about
half a minute. For speed, use the reference implementation, the `bread-format` Rust crate.

## Tests

```text
python -m unittest discover -s tests
```

run from this folder with `src` on `PYTHONPATH`, or after `pip install -e .`. They read every file
in [`conformance/`](../conformance).
