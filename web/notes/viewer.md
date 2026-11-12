---
title: The viewer and converter
status: stable
updated: 2026-11-12
---

A page that opens `.bread` files and turns STL into `.bread`, in the browser. The page hosts it at
`/viewer/`, from `viewer/` on main, built as that folder's README says. Everything happens on the
visitor's computer: files are never sent anywhere, and there is no server behind it.

**What it does:**

- **Open** a `.bread` file, dropped on the page or picked. Its models are placed as the scene says,
  on a millimetre grid as the build plate. It can be turned, moved and zoomed with the mouse or a
  finger.
- **Convert** STL files, binary or ASCII, into one `.bread` file, each STL a part named after its
  file. Several STL files make a kit in one file.
- **Check every conversion before offering it.** The new file is read back, and every corner of
  every triangle is compared, bit for bit, with the STL it came from. A file that does not give the
  STL back exactly is not offered. Then the page opens it, so what is shown is what was written.
- **Parts:** each part's name, triangles and size in millimetres, shown or hidden on its own.
- **Download as STL**, the parts shown or one of them, for programs that do not read `.bread` yet.
- **Supports:** a file's support previews, in the support colour, with a switch to hide them.

**How it is made.** It reads and writes through the reference implementation built to WebAssembly,
so it makes the same checks as every other reader. The module that reads is about 73 KB and loads
with the page. The one that also writes is about 171 KB and loads only when STL files come in.
Both sizes are with brotli. Files are read and written off the page's thread, so the page does not
freeze on a large one. The page has no other dependencies.

**How fast.** In the browser, a figure of 1.55 M triangles, 77.3 MB as binary STL, became a
6.2 MB `.bread` file, 12.4 times smaller, in 2.4 s, check included. The same file opens in about a
third of a second.

**For more.** Caliper, a mesh repair program by the same author, opens and saves `.bread` too
([caliper.md](caliper.md)). The page points to it.
