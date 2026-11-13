---
title: The page for a public launch
status: draft
updated: 2026-11-13
---

What the page says first, for people who have never heard of `.bread`. Every number is a fact of
`facts.toml`, named here by its key.

**The top of the page.**

- **Title:** Lossless STL replacement.
- **Below it:** `mz4250_ratio_overall` times smaller on 2,288 public miniatures, and
  `v2_corpus_ratio_min` to `v2_corpus_ratio_max` times on 22 other meshes, from 3D scans to CAD
  parts. Every vertex bit for bit. An open format, Apache-2.0. The range is said with the headline
  number, so no one reads 14× as a promise for every model.
- **Buttons, in this order:** Convert an STL (`/viewer/`) · See the measurements · Read the
  specification · GitHub (`project.repository`).
- **One line under them:** Runs in your browser. Your files never leave your computer.

**Who it is for**, three short blocks:

- **Designers and sculptors:** send a model to a client, or to the people who back your work, at a
  fraction of the size and without losing a bit; anyone can turn it back into STL.
- **Sites that host models:** less storage and less bandwidth, and an exact STL whenever someone
  asks for one.
- **Developers of tools:** a frozen specification, a reference implementation under Apache-2.0,
  WebAssembly for the browser, and chunks of your own that other programs skip
  ([implementers.md](implementers.md)).

**A box about the licenses:** the format, the specification and the code are Apache-2.0.
Breadslicer, the slicer it came from, is a separate program under the AGPL, and none of its terms
reach anyone who uses `.bread`.

**Questions** ([faq.md](faq.md)), and **for implementers** ([implementers.md](implementers.md)),
each a section of its own.

**The measurements**, one page: the comparison with zip, Zstandard, OBJ and 3MF
([formats.md](formats.md)), the corpus ([mz4250-corpus.md](mz4250-corpus.md)), how they were
measured ([measurements.md](measurements.md)), and the data files themselves, linked from the
repository. Each number says whether anyone can measure it again (it has `derive` in
`facts.toml`, checked by `check_facts.py`) or was measured once (its `source`).

**Shared links.** The title and description of the page say what the top says. The preview image
shows the headline number, "Lossless", and the name.

**Not decided yet,** and not shown until Damian decides: funding (issue #126) and counting visits
(issue #127).
