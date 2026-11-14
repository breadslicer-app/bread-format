# .bread viewer

A static web page that shows `.bread` files and turns STL into `.bread`, all in the browser: files
never leave the visitor's computer, and there is no server behind it. It reads and writes through
the JavaScript package in [`bindings/js`](../bindings/js), the reference implementation built to
WebAssembly, so it makes the same checks as every other reader.

- **Open** a `.bread` file, dropped on the page or picked, and see its models placed as the scene
  says, on a millimetre grid as the build plate, Z up. Drag to turn, right drag or shift to move,
  the wheel to zoom, a double click to frame everything. On a touch screen, one finger turns and two
  pinch and move.
- **Convert** STL files, binary or ASCII, dropped or picked, into one `.bread` file: each STL a part
  named after its file, where the STL put it. Before the file is offered it is read back, and every
  mesh is compared with what was read from its STL, and every corner of a binary STL with the STL's
  own bytes, bit for bit. Then the page opens it, so what is shown is what was written, and offers
  it to download with its size before and after.
- **Side by side**, after converting binary STL: the STL on the left and the `.bread` file on the
  right (one above the other on a screen taller than wide), turned, moved and zoomed by one camera.
  The STL side is read on the page straight from the file's bytes, without the format's code; the
  `.bread` side is the file just written, opened as any reader opens it. Triangles keep their
  order, so triangle *i* is the same on both. Under the pointer (or a tap) a loupe shows that place
  4 to 64 times larger on both sides, with each side's wireframe drawn from its own data and the
  triangle there lit; the panel shows that triangle's corners on both sides, as decimals and as the
  32 bits of each `f32`, and the STL's facet normal, which `.bread` does not store. Up to 8 million
  triangles, since both copies are on the graphics card; Escape goes back to one view.
- **Parts:** every leaf shown, with its name, its triangles and its size in millimetres in world
  positions; each can be shown or hidden, and lights up in the format's tone while the pointer is over it.
- **Download as STL**, for programs that do not read `.bread` yet: the parts shown as one binary
  STL, or one part, in world positions (each vertex moved by its leaf's transform in `f64`, then
  rounded to `f32`). A transform that mirrors keeps the faces facing out.
- **Supports:** a file's support previews, drawn where their support leaves are, in Breadslicer's
  support colour, with a switch to hide them.
- `?file=name.bread` in the address opens a file served next to the page, for links to examples.

The page is in English, and in Spanish for a browser that asks for it.

## How it is made

| File | What it does |
|---|---|
| `src/index.html`, `src/style.css` | the page, in the look of bread.synrgs.tech: its colours, Archivo and JetBrains Mono |
| `src/fonts/` | the two fonts, both under the SIL Open Font License (`OFL.txt` beside them) |
| `src/app.js` | the page's behaviour: files in, the panel, the words in both languages |
| `src/render.js` | a small WebGL2 renderer: flat shading worked out in the fragment shader, so a mesh of millions of triangles needs no normals or extra copy; the grid; the orbit camera |
| `src/worker.js` | reads and writes in a Web Worker, so the page never freezes on a large file |
| `src/core.js` | what is done with files, apart from the page, and tested in Node |

It has no dependencies. The reading module (`bread.wasm`, about 77 KB with brotli) loads with the
page; the writing module (`bread-write.wasm`, about 171 KB) only when STL files first come in.
`src/convert.js` is the converter's part of the page, `src/parts.js` the parts panel, `src/download.js` the STL download, `src/supports.js` the support previews, and `src/compare.js` the side-by-side comparison. Its triangle under the pointer is found on the graphics card: the STL's triangles drawn, numbered, into one pixel.

## Building and trying it

```sh
cd bindings/js && npm run build    # the WebAssembly modules (see its README)
cd ../../viewer && npm run build   # the page, whole, in viewer/dist
npm test                           # src/core.js against the conformance files
npm run serve                      # http://localhost:8080
```

`viewer/dist` is the whole page: its files, the package as `bread.js`, the modules, and the
`NOTICE` and `LICENSE` that go with them, and the fonts in `fonts/` with their `OFL.txt`. It can be
served as static files from any address; module workers and WebAssembly need http, not `file://`.
`.wasm` files should be served as `application/wasm`, and `.woff2` as `font/woff2`.
