---
title: The scene
status: stable
updated: 2026-11-11
---

The scene is a tree of groups and leaves. Every node has a transform, every leaf shows one mesh, and
anything used twice is stored once.

| Node | What it does |
|---|---|
| group | holds other nodes, in order |
| leaf | shows one mesh |
| shared leaf | shows a mesh the scene keeps as shared on purpose: editing it changes every leaf that shows it |
| reference | places one more copy of a whole subtree |

A mesh that several plain leaves show is also stored once. The result of an operation on several
meshes, such as a boolean difference, is stored as a leaf that shows the resulting mesh.

An example, a kit of parts:

```
roots
└─ group      "kit"
   ├─ leaf    "base"            → mesh 0
   ├─ leaf    "arm L"           → mesh 1
   ├─ leaf    "arm R"   M = −x  → mesh 1
   ├─ reference          T₁  → subtree 0
   └─ reference          T₂  → subtree 0

subtree 0
└─ group      "bolt"
   └─ leaf    "thread"          → mesh 2
```

"arm R" shows the same mesh as "arm L", mirrored, and it is stored once. Each reference places one
more bolt.

**Transforms.** A node's transform is a 3 × 3 matrix $M$ and a translation $t$, in `f64`. It takes a
point from the node's own space to its parent's:

$$p' = M p + t \iff \begin{pmatrix} x' \\ y' \\ z' \end{pmatrix} = \begin{pmatrix} m_{00} & m_{01} & m_{02} \\ m_{10} & m_{11} & m_{12} \\ m_{20} & m_{21} & m_{22} \end{pmatrix} \begin{pmatrix} x \\ y \\ z \end{pmatrix} + \begin{pmatrix} t_x \\ t_y \\ t_z \end{pmatrix}$$

A node stored without a transform has the identity.

**Placing the meshes.** Walk down from each root, in order, starting with the identity. Every node's
world transform is its parent's composed with its own, and a leaf sends each vertex $v$ of its mesh
to $W(\text{leaf})(v)$:

$$W(\text{node}) = W(\text{parent}) \circ T(\text{node}), \qquad (A \circ B)(p) = A(B(p))$$

A reference goes on into its subtree's root as if it were its only child. A subtree that no
reference names is not shown, and neither is a mesh that no leaf shows.

Transforms are `f64` and vertices `f32`, so world positions are computed in `f64` and rounded to
`f32` only at the end, if at all.

**No cycles, without looking for them.** Nodes are listed children first: every node names only
nodes, meshes and subtrees listed before it. So no node can contain itself, and no subtree can
reach itself through references. A reader proves it just by reading in order. It still checks that
each node has at most one place: one group's child, a root, or a subtree's root.
