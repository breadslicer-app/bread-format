# bread

A command-line program that converts STL to `.bread` and back, and looks inside `.bread` files,
on the reference implementation in [`crates/bread-format`](../bread-format).

```
bread model.stl                         # model.bread, next to it
bread model.bread                       # model.stl, next to it
bread from-stl a.stl b.stl -o kit.bread --name "A kit"
bread to-stl kit.bread -o kit.stl
bread info kit.bread
bread check kit.bread other.bread
```

Given files alone, `bread` converts each by its extension and writes the result next to it, so on
Windows `.stl` and `.bread` files can be dropped on `bread.exe`. It does not write over a file
that is already there unless told to with `--force`.

- **`from-stl`** makes STL files, binary or ASCII, into one `.bread` file, as the
  [viewer](https://bread.synrgs.tech/viewer/) does: each STL a part named after its file, where
  the STL put it. Before the file is written, it is read back and checked: every mesh as read from
  its STL, and every corner of a binary STL as the STL's own bytes, bit for bit. A file that fails
  the check is not written.
- **`to-stl`** makes everything a `.bread` file shows into one binary STL, as the viewer's
  "Download as STL": each part in its world position, each vertex moved by its transform in f64
  and rounded to f32 (SPEC 5.4). Parts with no transform keep their vertices bit for bit. Support
  leaves show their support preview, when the file has one.
- **`info`** lists a file's chunks, its meshes, the parts it shows and the slicer data it keeps.
- **`check`** reads a file as every reader must, and says why it refuses it, if it does. It exits
  with 1 when a file is refused.

Exit codes: 0 when everything went well, 1 when a file failed, 2 for wrong arguments.

## Build

```
cargo build --release -p bread-cli    # target/release/bread(.exe)
cargo test -p bread-cli
```

The tests run the program on every file in [`conformance/`](../../conformance): `check` reads
every valid file and refuses every invalid one, and every valid file goes to STL, back to
`.bread` and to STL again with the same corners, bit for bit.

## License

Apache-2.0. The program has meshoptimizer and Zstandard built in: their notices are in
[`NOTICE`](NOTICE), and go with every copy of it.
