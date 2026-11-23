# bread-thumbnails

Thumbnails of `.bread` files in Windows Explorer. This crate builds
`bread_thumbnails.dll`, a shell thumbnail provider (`IThumbnailProvider`): once
it is registered for `.bread`, Explorer shows the picture in a file's thumbnail
chunk (`THMB`, section 8 of [`SPEC.md`](../../SPEC.md)), and the type's icon for
a file that has none. It works on Windows 10 and 11, 64-bit.

## What it shows

- **The thumbnail in the first chunk.** Section 8.1 asks writers to put the
  thumbnail first, so that a program that only wants the picture reads the
  first chunk and stops. The provider does exactly that: it never reads,
  decompresses or draws the model, however large the file. A file whose first
  chunk is not a thumbnail gets the type's icon.
- **Checked like any chunk.** The header, the chunk's CRC, its single Zstandard
  frame and its raw length are checked by the `bread-format` crate's reader. A
  chunk longer than 4 MiB, a thumbnail chunk of a version other than 1, a PNG
  larger than 512 × 512 (read from its header, before decoding, as section 8.2
  asks) or one that does not decode all give the icon instead.
- **Scaled to the size Explorer asks for**, keeping its shape, with
  transparency kept (`WTSAT_ARGB`).

## Building

```
cargo build -p bread-thumbnails --release
```

For a DLL to hand out, also link the C runtime statically, so that it needs no
Visual C++ redistributable, and keep the building machine's paths out of it:

```
RUSTFLAGS="-C target-feature=+crt-static --remap-path-prefix=<cargo home>=/cargo --remap-path-prefix=<checkout>=/bread-format"
```

The DLL imports only system libraries.

## Registering

The download on the [releases page](https://github.com/breadslicer-app/bread-format/releases)
has the DLL with `register.cmd` and `unregister.cmd`, which do the `regsvr32` steps below for the
current user, without administrator rights.

The class id is `{AF9572F8-199D-4710-88C0-5D553D38F093}`. A provider needs two
registry keys, under `HKEY_LOCAL_MACHINE\Software\Classes` for every user or
`HKEY_CURRENT_USER\Software\Classes` for one:

| Key | Value | Data |
|---|---|---|
| `CLSID\{AF9572F8-199D-4710-88C0-5D553D38F093}` | (default) | `Bread thumbnail provider` |
| `CLSID\{AF9572F8-199D-4710-88C0-5D553D38F093}\InprocServer32` | (default) | the DLL's full path |
| | `ThreadingModel` | `Apartment` |
| `.bread\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` | (default) | `{AF9572F8-199D-4710-88C0-5D553D38F093}` |

The last key's name is the shell's own id for thumbnail handlers. An installer
writes these keys itself, and removes them on uninstall.

For testing, `regsvr32 bread_thumbnails.dll` writes them for the current user,
with the path of the DLL it is given, and `regsvr32 /u bread_thumbnails.dll`
removes them. Explorer keeps thumbnails in a cache: a file it already showed
with the icon may keep it until the cache is cleared (Disk Cleanup,
"Thumbnails").

## Checking it

`cargo test -p bread-thumbnails` tests the reading, decoding and scaling on
every platform, and on Windows the provider through its COM interfaces.

`examples/shell_thumbnail.rs` asks Windows for a file's thumbnail the way
Explorer does, through the shell and whatever provider is registered, and
writes it as a PNG:

```
cargo run -p bread-thumbnails --example shell_thumbnail -- model.bread out.png 256
```

## Licence

Apache-2.0. The DLL includes Zstandard and meshoptimizer, whose notices are in
[`NOTICE`](NOTICE).
