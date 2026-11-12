# .bread viewer

A static web page that shows `.bread` files and turns STL into `.bread`, all in the browser: files
never leave the visitor's computer, and there is no server behind it. It reads and writes through
the JavaScript package in [`bindings/js`](../bindings/js), the reference implementation built to
WebAssembly, so it makes the same checks as every other reader.

- **Open** a `.bread` file, dropped on the page or picked, and see its models placed as the scene
  says, on a millimetre grid as the build plate, Z up. Drag to turn, right drag or shift to move,
  the wheel to zoom, a double click to frame everything. On a touch screen, one finger turns and two
  pinch and move.
- `?file=name.bread` in the address opens a file served next to the page, for links to examples.

The page is in English, and in Spanish for a browser that asks for it.

## How it is made

| File | What it does |
|---|---|
| `src/index.html`, `src/style.css` | the page, in Breadslicer's colours |
| `src/app.js` | the page's behaviour: files in, the panel, the words in both languages |
| `src/render.js` | a small WebGL2 renderer: flat shading worked out in the fragment shader, so a mesh of millions of triangles needs no normals or extra copy; the grid; the orbit camera |
| `src/worker.js` | reads and writes in a Web Worker, so the page never freezes on a large file |
| `src/core.js` | what is done with files, apart from the page, and tested in Node |

It has no dependencies. The reading module (`bread.wasm`, about 73 KB with brotli) loads with the
page.

## Building and trying it

```sh
cd bindings/js && npm run build    # the WebAssembly modules (see its README)
cd ../../viewer && npm run build   # the page, whole, in viewer/dist
npm test                           # src/core.js against the conformance files
npm run serve                      # http://localhost:8080
```

`viewer/dist` is the whole page: its files, the package as `bread.js`, the modules, and the
`NOTICE` and `LICENSE` that go with them. It can be served as static files from any address; module
workers and WebAssembly need http, not `file://`. `.wasm` files should be served as
`application/wasm`.
