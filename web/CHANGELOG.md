# Changelog of the corpus

Newest first.

## 2026-11-13, the numbers are checked against their data

- 34 facts have `derive`: how each is computed from `data/figure-sizes.json`,
  `data/mz4250-corpus.json`, the conformance files or `python/pyproject.toml`.
  `check_facts.py` computes them in CI and fails if a value written is not its data rounded; all 34
  are right. It also lists the 39 facts measured once, with their sources.
- New field, `derive`, on facts. The page can ignore it, or show which numbers anyone can check.

## 2026-11-13, the repository is public

- `project.repository` is https://github.com/breadslicer-app/bread-format, public since
  `project.repository_public_on`, 2026-11-13. The page can link to it.
- `rules.md`, rule 4: links to the repository are allowed now, to `main` or a tag. The viewer is
  still served as a copy of `viewer/dist`.

## 2026-11-12, the viewer is online

- `[tools.viewer]`: `released = true`, `released_on = 2026-11-12`: the page serves it at `/viewer/`.
- `viewer_reader_kb` is 77 (was 73) and `web_reader_kb` 77 (was 71): the reading module grew with
  the scene and the support previews. `viewer_writer_kb` stays 171. Sources give the bytes of main
  `6e3cd2f`. Sizes are decimal: 76,846 bytes is 77 KB (75.0 KiB).
- The viewer links back to the page from its title.

## 2026-11-12, the viewer and converter

- **The viewer**: a page that opens `.bread` and turns STL into `.bread`, in the browser. It is
  described in `notes/viewer.md`, with `[tools.viewer]` (`released = false` until the page serves
  it at `/viewer/`) and the facts `viewer_*`.
- `rules.md`, rule 4: the page serves the viewer as a copy of `viewer/dist`, not as a link.

## 2026-11-12, Caliper 0.9.6

- `[implementations.caliper]`: `released = true`, `version = "0.9.6"`, `released_on = 2026-11-11`.
  Caliper 0.9.6 is the first release that opens and saves `.bread`.

## 2026-11-12, a third implementation

- **Caliper** opens and saves `.bread`: a C++ implementation written from the specification alone.
  It is in `notes/caliper.md`, in `[implementations.caliper]` and in the facts `caliper_*`. It is
  not in a release of Caliper yet (`released = false`).
- `rules.md`, rule 3: the author's own programs, Breadslicer and Caliper, are named.

## 2026-11-11, version 1 is frozen

- `spec_status` is `"frozen"`: the tag `spec-v1` exists.
- `rules.md`, rule 5: the page may now quote `SPEC.md` as of `spec-v1`, show its layouts and offer
  the conformance files, hosted on the page. Nothing newer than the tag.
- `notes/status.md`: the read-through is done.
- `[project] version` is `0.3.0`, the release of the reference implementation, with `0.2.0`
  moved to `earlier_versions`. It had stayed at `0.2.0`.

## 2026-11-11, one Zstandard frame

- `conformance_invalid` is 19: a reader now also refuses a chunk stored as two Zstandard frames in
  a row (`invalid/two-frames`).

## 2026-11-11, a public corpus

- **MZ4250's miniatures:** 2,288 models under CC BY 4.0, all measured with the reference
  implementation 0.3.0. They are in `data/mz4250-corpus.json`, with one row per model, in
  `notes/mz4250-corpus.md`, and in the facts `mz4250_*`, with the credit in `[credit.mz4250]`.
- `rules.md`: these models are named, always with that credit. It is the one exception to "no
  model names".

## 2026-11-11, a key for the 1.97 M mesh

- New fact `js_mesh_triangles` (1,967,357): the mesh the JavaScript times and the second reader's
  time use. Until now it only lived in the `what` of `open_js_v2_ms` and `second_reader_open_v2_s`.

## 2026-11-11, later

- `conformance_invalid` is 18: a reader now also refuses container version 0.
- **The second reader is done:** `notes/second-reader.md`, and the facts `second_reader_spec_gaps`,
  `second_reader_open_v2_s` and `second_reader_dependencies`. `notes/status.md` ticks it off.
- `notes/status.md`: the freeze does not wait for Breadslicer.

## 2026-11-11

The first corpus, for the page at bread.synrgs.tech.

- `facts.toml`: every number on the page, with its source and date.
- **The figure's sizes, measured again** (`data/figure-sizes.json`, `notes/measurements.md`). The
  page's earlier numbers came from older measurements, done in different ways. These are the ones
  to use:

  | Key | Was on the page | Now |
  |---|---|---|
  | `figure_obj_mb` | 73.1 MB | 69.7 MB |
  | `figure_stl_zip_mb` | 41.2 MB | 39.3 MB |
  | `figure_stl_7z_mb` | 33.2 MB | 32.2 MB |
  | `figure_obj_zstd19_mb` | 15.8 MB | 12.4 MB |
  | `figure_bread_v1_mb` | 8.5 MB | 7.8 MB |
  | `figure_bread_v2_mb` | 7.6 MB | 6.6 MB |
  | `figure_stl_over_bread` | "a twelfth" | 14.4 × |

  The binary STL stays at 94.9 MB.
- **Real projects:** `real_projects_change_min` and `_max` are −8.8 and −36 %, not "10–35 %".
- **Times:** `open_native_v1_ms` is 39 ms, measured again, not "~50 ms". `save_v2_s` is 0.43 s.
  The JavaScript times stay as they are (`open_js_v2_ms`, `open_js_v2_first_ms`).
- **Gone:** "−1 to −8 % on CAD parts" has no measurement behind it in the corpus, so it is not a
  fact. `notes/measurements.md` says CAD parts gain the least, without a number.
- `notes/`: what it is, the container, the scene, the mesh, prediction, entropy coding, safety,
  slicer data, conformance, measurements and status.
- `rules.md`: what is not published until the tag `spec-v1`.
