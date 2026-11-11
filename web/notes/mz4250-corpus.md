---
title: A public corpus, the miniatures of MZ4250
status: stable
updated: 2026-11-11
---

**Models by MZ4250**, shared under CC BY 4.0: [Printables](https://www.printables.com/@MZ4250) ·
[mz4250.com](https://mz4250.com/) · [the license](https://creativecommons.org/licenses/by/4.0/).
They were only measured, and none is redistributed here.

MZ4250 shares thousands of tabletop miniatures under a licence that lets anyone use them, with credit. That makes
them a corpus anyone can check: download the same models, convert them, and compare. The figures
elsewhere on the page come from private projects, which no one else can measure again.

**What was measured:** 2,288 binary STLs, 162 GB in all, from 25 thousand to 11.9 million
triangles. For each model, one at a time, on a desktop PC, with the
reference implementation 0.3.0 (`tools/corpus-bench` in the repository):

1. the STL's corners are welded into vertices: two corners are one vertex when their coordinates are
   bit for bit equal, numbered by first use, as an STL importer welds them;
2. the mesh is written as a `.bread` file of one leaf, in memory, and timed;
3. it is read back three times, and the fastest is kept;
4. every corner of every triangle is compared with the STL's, bit for bit and in order.

**What came out:**

- every model came back bit for bit: all 3.24 billion triangles, each corner exactly as the STL had
  it;
- 162 GB of STL became 11.6 GB of `.bread`, 14 times smaller overall. Model by model, from 7.7 to
  55.7 times, with a median of 15.1;
- writing goes at about 2.6 M triangles a second, and reading at about 11.9 M. The median model,
  1.2 M triangles, writes in 0.45 s and reads in 0.1 s;
- the largest, Treasure Golem, 11.9 M triangles, goes from 596 MB of STL to 24 MB, writes in 4.2 s
  and reads in 0.8 s.

The STL keeps a facet normal and two attribute bytes with every triangle, which `.bread` drops as
file data (see `what-it-is.md`). Everything else, every coordinate and every corner's place, is
kept.

**Where it gains least and most.** The least: *Owlbears Running 1* (7.7 times), *Bugbear
Barbarian 2* (7.9) and *Giant Posionous Snake* (8.2). The most: *Tri-Flower Frond* (55.7),
*Animated Spoon* (35.0) and *Warrior Goddess Avatar* (34.4).

**The second reader.** A sample of the corpus was read again with the second reader, the plain
Python one written from the specification alone. Its corners were compared with the STLs directly,
without the reference implementation (`facts.toml`: `mz4250_second_reader_sample`).

The full table, one row per model with its name, triangles, sizes and times, is in
`data/mz4250-corpus.json`. Each model is named by its STL's file name, spelling included.
