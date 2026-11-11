# What is never published

These rules come before everything else in `web/`. Each one says why.

1. **No model names or file names.** The models measured are third parties', or clients', and one
   is a 3D scan of a real person. Name them generically: "a figure of 1.9 M triangles", "a
   sculpture", "a CAD part", "the supports of a large model".

   **The one exception is MZ4250's corpus** (`notes/mz4250-corpus.md`, `data/mz4250-corpus.json`).
   Those models are shared under CC BY 4.0, so they are named, by their file names, and always with
   their credit: "Models by MZ4250", a link to https://www.printables.com/@MZ4250 and to
   https://mz4250.com/, and a link to the licence, https://creativecommons.org/licenses/by/4.0/.
   One credit for the whole corpus is enough; no link per model. The models themselves, their
   files, renders or pictures, are not shown.
2. **No paths and no private research.** Nothing from disk paths, private folders, prototypes or
   test benches beyond what `web/` says.
3. **No competing products.** Slicers and other products are not named. Other file formats are:
   STL, OBJ, 3MF, glTF. Libraries the format stands on are named with credit: Zstandard,
   meshoptimizer (by Arseny Kapoulkine).
4. **No links to the repository.** It is private: a link would give the public a 404.
5. **The specification as frozen, and nothing newer.** Version 1 is frozen (tag `spec-v1`), so the
   page may quote `SPEC.md` and show its layouts, such as tables of field offsets, and may offer the
   conformance files as downloads hosted on the page itself (rule 4 still holds). Only the text as
   of the tag `spec-v1`: a change made after it is shown once a new version of the specification
   is tagged. Wherever the page quotes it, it says it is version 1.
6. **No release that has not happened.** The crate is not on crates.io, the JavaScript package is
   not on npm, and the Python reader is not on PyPI. The page does not say they are, or link to
   them, until `facts.toml` says so.
