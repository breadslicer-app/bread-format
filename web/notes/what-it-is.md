---
title: What .bread is, and who it is for
status: stable
updated: 2026-11-11
---

`.bread` is an open file format for triangle meshes and the scene that places them, stored as small
as they can be without losing anything. It is meant to replace STL wherever a model goes from one
program, or one person, to another.

**Lossless** means that a mesh read back is the mesh that was written:

- every vertex coordinate, bit for bit, including negative zero, subnormals and NaN payloads;
- every triangle, in the same order, starting at the same corner, so winding and first corner are
  both kept.

**What a file holds:**

- one or more meshes;
- a scene: named parts, groups, meshes shared between parts, copies of whole subtrees, and a
  transform on every node;
- room for a slicer: editable supports, hollowed models, paint on faces, a thumbnail, and a preview
  of the supports for readers that cannot rebuild them.

**Left out, on purpose:**

- colours, textures, UV coordinates, materials and animation. That is glTF's job, and `.bread` does
  not compete with it;
- the facet normals an STL stores with every triangle. They are file data, not geometry: any program
  computes them from the corners.

**Units and axes.** Lengths are millimetres. Axes are right-handed, with Z up and the XY plane as
the build plate. Seen from outside, a triangle's corners should go counter-clockwise, as in STL.
There is no unit field: like STL, the format fixes one.

**Who it is for.** Designers who hand models to someone else, such as a client, and anyone who wants
to take a pre-supported model out of a slicer with its supports still editable.

**Where it comes from.** It started as the project file of Breadslicer, a resin slicer, and is now a
format of its own. It has its own specification and a reference implementation, both Apache-2.0;
Breadslicer stays AGPL. The application chunks (`PROJ`, `SUPP`, `HOLW`) are Breadslicer's. Any
other program can skip them and still read the same meshes in the same scene.

**The name.** BREAD stands for *Bit-exact Representation of Editable Assembly Data*. It is also just
bread: Breadslicer slices it, and the slices are the layers.

**Standing on.** Zstandard (RFC 8878) for the container; the attribute and triangle codecs of
meshoptimizer, by Arseny Kapoulkine, as published in glTF's `KHR_meshopt_compression`; CRC-32 as in
zlib and PNG; the requirement words of RFC 2119 and RFC 8174.
