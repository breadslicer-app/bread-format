# bread-format

Reads and writes `.bread` files: triangle meshes and the scene that places them, as small as they
can be stored without losing anything, with room for a slicer's data such as editable supports.

- **Lossless:** every vertex coordinate comes back bit for bit, and every triangle in its order,
  from its first corner.
- **Small:** 2,288 binary STL miniatures, 162 GB in all, became 11.6 GB of `.bread`, every one of
  them bit for bit ([how it was measured](https://github.com/breadslicer-app/bread-format/blob/main/web/notes/mz4250-corpus.md)).
- **Open:** the format is described in
  [`SPEC.md`](https://github.com/breadslicer-app/bread-format/blob/main/SPEC.md), version 1, frozen,
  with conformance files that any reader can check itself against. Specification and code are
  Apache-2.0.

```rust
let file = bread_format::read_file(std::fs::File::open("model.bread")?)?;
for mesh in &file.scene.meshes {
    // vertices: [f32; 3] each; triangles: three vertex indices each.
    println!("{} vertices, {} triangles", mesh.vertices.len(), mesh.triangles.len());
}
```

`read_stl` reads an STL file into a mesh, and `write_file` writes a `.bread` file. Files can be
opened and converted in the browser at <https://bread.synrgs.tech/viewer/>, and the format's page
is <https://bread.synrgs.tech>.

## License

Apache-2.0 (`LICENSE`). The meshoptimizer and Zstandard sources built into the crate come with
their own notices (`NOTICE`).
