---
title: The mesh, and the order of its triangles
status: stable
updated: 2026-11-11
---

A mesh is its vertices, three `f32` each, and triangles that name them by index. A writer may not
reorder anything to compress better: vertices and triangles stay in the mesh's own order.

## Version 1

Version 1 stands on two published bitstreams, the ones meshoptimizer wrote for glTF
(`KHR_meshopt_compression`):

- the vertices go through the **attribute codec**. It works on bytes and keeps every one, so it
  keeps every bit of every coordinate;
- the triangles go through the **triangle codec**.

The triangle codec may hand a triangle back starting at another corner: $(a, b, c)$ can come back
as $(b, c, a)$ or $(c, a, b)$, with the same winding. Version 1 stores a short list of the triangles
that came back turned, and turns them back.

## Version 2

Version 2 keeps the same mesh in fewer bytes, in two ways:

- the triangles can be stored in an order the triangle codec likes, with the way back to their own
  order;
- the vertices can be predicted from the triangles around them ([prediction.md](prediction.md)), so
  that only the difference is stored.

A version 2 chunk names its **connectivity**, A or B, and its **vertex coding**:

| Vertex coding | What it stores |
|---|---|
| 0, meshopt | the vertices through the attribute codec, as in version 1 |
| 1, rANS | the residuals of the prediction, entropy coded ([entropy-coding.md](entropy-coding.md)) |
| 2, byte planes | the residuals as 12 planes of bytes, left for the chunk's Zstandard to squeeze |

Some parts are **bit streams**: values packed least significant bit first, exactly as long as
their bits need, with the unused bits of the last byte at zero. A reader refuses any other length.

## Triangle order

The **decoding order** is the order the triangles are stored in. A **corners** part, 2 bits per
triangle in decoding order, says which corner of each decoded triangle comes first:

$$(s_0, s_1, s_2) \xrightarrow{\ r\ } (s_r,\ s_{r+1},\ s_{r+2}), \quad \text{indices mod } 3$$

The value 3 is refused.

**Connectivity A.** The triangles are in their own order, and the vertices are numbered as the mesh
numbers them. It works for any mesh.

**Connectivity B.** The triangles are in the order the writer picked, usually the one
meshoptimizer's vertex cache optimizer gives, plus a **permutation** back: one value $p_j$ per
decoded triangle $j$, the triangle's index in its own order, each in just enough bits for $t$
triangles:

$$w = \operatorname{bitlen}\big(\max(t, 2) - 1\big)$$

To read the mesh:

1. put each decoded triangle $j$, turned by its corner, at index $p_j$;
2. number the vertices by first use: walk the triangles in their own order, corners in order; the
   first vertex met is 0, the next new one 1, and so on;
3. give each vertex that number, keeping its coordinates, and rename the corners the same way.

So B only round-trips a mesh whose vertices were already numbered by first use, every vertex used,
and a writer may use it for nothing else. Meshes welded from an STL are numbered exactly that way.
The reference writer tries B only when A's index buffer takes more than 2 bytes per triangle, and
keeps it when it stores the triangles smaller.
