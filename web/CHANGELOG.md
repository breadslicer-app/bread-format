# Changelog of the corpus

Newest first.

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
