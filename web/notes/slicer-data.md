---
title: Slicer data
status: stable
updated: 2026-11-11
---

These chunks are what turns a mesh into a print job. Any program can skip all of them and still show
the same models in the same places.

| Chunk | What it holds |
|---|---|
| `SUPP` | Editable supports. Each entry names a model and the leaf that shows its supports. Its document holds tips, trunks, joints, rafts and the settings they were built with. The support leaf is stored with an empty mesh and rebuilt from the document, so the supports stay editable. |
| `HOLW` | Hollowed models. The model's leaf shows the hollow, which is what prints. The entry names the original mesh, and the document keeps the hollowing settings and the drain holes, so the model can be hollowed again with new ones. |
| `SPRV` | A preview of the supports: their mesh as it was when the file was written, so that a viewer or a shop that cannot rebuild supports still shows them. It is for showing, never for editing. |
| `SURF` | Paint on faces: details to keep, or where supports must or must not touch. One mask per leaf and kind, as runs of a value from 0 to 255 over the triangles, the values in between for a soft brush. |
| `THMB` | A thumbnail: a PNG of up to 512 × 512 in sRGB, written first so that a file manager reads one chunk and stops. |
| `PROJ` | Breadslicer's project settings, as a versioned TOML document: printer, resin and export format. |

The format describes only the envelope of `PROJ`, `SUPP` and `HOLW`: which parts of the scene a
document is about, the role each part plays, and who wrote it. What a document holds belongs to the
program that wrote it.

**Paint never lands on the wrong mesh.** Every paint mask carries the triangle count and a
fingerprint of the mesh it was painted on: a SHA-384 over its geometry, all little-endian, without
its name. If the leaf's mesh has changed, the fingerprint no longer matches, and the paint is
ignored instead of being misplaced.

$$\text{fingerprint} = \operatorname{SHA\text{-}384}\big(\,n_\text{u64} \,\|\, x_0 y_0 z_0 \ldots \,\|\, t_\text{u64} \,\|\, a_0 b_0 c_0 \ldots\big)$$

with the coordinates as the bits of their `f32` and the corners as `u32`, first corner first.
