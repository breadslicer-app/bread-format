# Bread Converter

A desktop program that converts STL to `.bread` and back, many files at once, in a window drawn
with [Breadslicer](../../README.md#breadslicer)'s own controls and colours, so it belongs to the
family. It is built on [`crates/bread-cli`](../../crates/bread-cli): each file is converted and
checked exactly as `bread from-stl` and `bread to-stl` do it.

- **Add** files, or folders with or without their subfolders, with the buttons or by dropping
  them on the window. Files given on the command line are added too, so `.stl` and `.bread` files
  can be opened with it.
- **Convert** both ways, each file by its extension, or only STL to `.bread`, or only `.bread` to
  STL. Every `.bread` file is read back and checked bit for bit against its STL before it is
  written; one that fails the check is not written.
- **Write** next to each file, or into a folder, keeping the subfolders the files were found in
  or not.
- **If the file is there:** skip it, replace it, or keep both (`name (2).bread`). Two files never
  write the same file, and no file is written over another file of the list.
- Up to four files are converted side by side. The list shows each file's size before and after,
  and why a file failed. Files are written to a `.part` file first and moved into place once
  whole, so stopping never leaves half a file.
- **Stop** stops after the files being converted. **Open folder** opens where the files went.
- The window remembers its choices (`%APPDATA%\Bread Converter\settings.txt` on Windows), and
  speaks English and Spanish.

## Build

```
cargo build --release -p bread-converter    # target/release/bread-converter(.exe)
cargo test -p bread-converter
```

On Linux the window needs fontconfig: its development files to build, or `RUST_FONTCONFIG_DLOPEN=on`
to load it when the program runs, as CI does.

The tests cover the queue without the window: which files folders give, where each goes, what
happens when a file is there, and the conversions both ways.

The window is `ui/app.slint`, on `ui/kit/`: Breadslicer's design tokens (`theme.slint`), controls,
dialog pieces and icons, as Breadslicer has them. The Spanish texts are in `tools/translate_es.py`;
after changing the texts of the window, run

```
slint-tr-extractor ui/app.slint -o i18n/bread-converter.pot
python tools/translate_es.py
```

`tools/icon.py` draws the icon, `assets/icon.png` and `assets/icon.ico`.

## License

Apache-2.0. The program carries meshoptimizer, Zstandard and the Noto Sans font, and draws its
window with [Slint](https://slint.dev) under Slint's royalty-free license for desktop
applications, whose attribution the About box shows: see [`NOTICE`](NOTICE).
