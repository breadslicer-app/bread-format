---
title: Against zip, Zstandard, OBJ and 3MF
status: stable
updated: 2026-11-13
---

**The question.** "Why not just compress the STL?" A fair one, so here is the measurement, on models
anyone can download.

**The models.** A sample of the miniatures of MZ4250 (CC BY 4.0, see `mz4250-corpus.md`): every
tenth STL in name order, 229 models and 327 million triangles, the same sample on every run. Each
was stored nine ways, all of them lossless:

| Way | GB | `.bread` is smaller by | Write, s | Read, s |
|---|---|---|---|---|
| `.bread` | 1.14 | | 129 | 28 |
| OBJ + Zstandard 19 | 2.61 | 2.3× | 3,698 | 58 |
| OBJ + gzip 9 | 3.56 | 3.1× | 890 | 53 |
| 3MF | 4.21 | 3.7× | 401 | 303 |
| STL + Zstandard 19 | 6.56 | 5.7× | 1,604 | 92 |
| STL + Zstandard 3 | 7.26 | 6.3× | 22 | 89 |
| STL in a zip, level 9 | 7.53 | 6.6× | 327 | 96 |
| OBJ | 12.52 | 10.9× | 57 | 32 |
| binary STL | 16.37 | 14.3× | 11 | 66 |

Sizes are decimal (1 GB = 1,000,000,000 bytes); times add up the 229 models.

**What came out:**

- every one of the 229 models came back bit for bit from all nine, every corner of every triangle
  in its place;
- `.bread` is the smallest of the nine for every one of the 229 models, not only in total;
- the nearest is the OBJ with Zstandard at its level 19, still 2.3 times larger, and it takes
  29 times longer to write;
- `.bread` is also the fastest to read back: 28 s for the whole sample, against 32 s for plain OBJ
  and 303 s for 3MF;
- writing is where `.bread` is not the fastest: plain STL and STL with Zstandard at level 3 write
  faster, since they barely transform the data, but they are 14.3 and 6.3 times larger.

**How each way was written.** All of it is in `tools/corpus-bench` (`--formats`):

- **binary STL:** written from the mesh, with each triangle's facet normal computed from its corners;
- **zip, gzip, Zstandard:** the STL's or the OBJ's bytes, through the usual libraries at the levels
  named. Zstandard runs on all 16 threads, as `zstd -T0`; zip and gzip on one;
- **OBJ:** a `v` line per vertex and an `f` line per triangle, each coordinate the shortest decimal
  that reads back as the same `f32`: the fewest digits that still lose nothing;
- **3MF:** its model file, with the same decimals, in a zip at Deflate's default level, level 6;
- **`.bread`:** as the reference implementation 0.3.0 writes it, some steps side by side on threads.

Each was written once and read back into triangle corners three times, the fastest kept, then
compared with the STL corner by corner.

**What this does not say:**

- the OBJ and 3MF here are lossless only because every coordinate has enough digits. Many writers
  round them, and lose bits; those files are smaller, but they are not the same model;
- the readers for STL, OBJ and 3MF are plain ones written for this measurement, so a faster
  library could read them faster. The sizes do not depend on that;
- lossy mesh compressors are not in the table: they move the vertices, which `.bread` does not;
- the run took two sittings on a desktop PC, an AMD Ryzen 7 5800X with 32 GB (see
  `measurements.md`), with other work on it at times. Other work slows the times down, but not the
  sizes.

**Repeating it.** With the models downloaded into one folder:

```
cargo run --release -p corpus-bench -- <folder> mz4250-formats.json --every 10 --formats
```

It measures the same 229 models. The results are in `data/mz4250-formats.json`, one row per model
with every format's bytes, times and check, and in `data/mz4250-formats.csv`, one line per model
and format.
