# corpus-bench

Measures `.bread` on a folder of binary STL files.

```text
cargo run --release -p corpus-bench -- <folder> <out.json> [--limit N]
```

For each STL, in name order, it:

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

Models are measured one at a time, so each one gets the whole machine, threads included. Other work
on the machine slows the times down, but not the sizes.
