# corpus-bench

Measures `.bread` on a folder of binary STL files.

```text
cargo run --release -p corpus-bench -- <folder> <out.json> [--limit N] [--every N] [--formats] [--zstd-threads N]
```

For each STL, in name order, it (with `--every N`, only the first of every N: a sample that is the
same on every run):

1. welds the triangle corners into vertices, two corners being one vertex when their coordinates are
   bit for bit equal, numbered by first use, as an STL importer welds them;
2. writes a `.bread` file of one leaf showing that mesh, in memory, and times it;
3. reads it back three times and keeps the fastest;
4. checks that every corner of every triangle comes back bit for bit, in the STL's order.

Nothing is written to disk but the results. Each model's row goes to `<out.json>.partial` as soon as
it is measured, so a run that stops can go on where it left off: run it again and the models already
there are skipped. At the end, `<out.json>` gets every row, sorted by name:

| Field | Meaning |
|---|---|
| `name` | the STL's file name, without `.stl` |
| `triangles`, `vertices` | the welded mesh |
| `stl_bytes`, `bread_bytes` | the two file sizes |
| `save_ms` | `write_file`, once |
| `open_ms` | `read_file`, the fastest of three |
| `bit_exact` | every corner came back bit for bit |

## Against other formats

With `--formats`, each mesh is also stored these other ways, each written so that it loses nothing,
and each is timed the same way (written once, read back into corners three times, the fastest kept)
and checked corner by corner against the STL:

| Key | What |
|---|---|
| `stl` | binary STL, written from the mesh with its facet normals |
| `stl_zip_9` | the binary STL in a zip, Deflate at level 9 |
| `stl_zstd_3` | the binary STL with Zstandard at level 3, its default |
| `stl_zstd_19` | the binary STL with Zstandard at level 19 |
| `obj` | OBJ, every coordinate as the shortest decimal that reads back as the same `f32` |
| `obj_gzip_9` | that OBJ with gzip at level 9 |
| `obj_zstd_19` | that OBJ with Zstandard at level 19 |
| `3mf` | 3MF: its model, with the same decimals, in a zip at Deflate's default level 6 |

Zstandard runs on `--zstd-threads` threads, all the machine's by default, as `zstd -T0`; zip, gzip
and 3MF on one thread; `.bread` as the reference implementation runs it, some steps side by side.
Each row then has a `formats` object, `{"<key>": {"bytes", "write_ms", "read_ms", "bit_exact"}}`,
and `<out>.csv` gets one line per model and format, `.bread` included. The summary at the end gives
each format's total and how much smaller `.bread` is than it.

OBJ and 3MF are lossless here only because every coordinate is written with enough digits. A writer
that rounds them, as many do, loses bits; those files are not measured.

Models are measured one at a time, so each one gets the whole machine, threads included. Other work
on the machine slows the times down, but not the sizes.
