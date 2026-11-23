# Releasing the programs

[`.github/workflows/release.yml`](../../.github/workflows/release.yml) builds the programs for
download and attaches them to a draft release:

| Archive | Platforms | Holds |
|---|---|---|
| `bread-<version>-<platform>` | Windows x64 (`.zip`), Linux x64 and macOS arm64 (`.tar.gz`) | the command-line program ([`crates/bread-cli`](../../crates/bread-cli)) |
| `Bread-Converter-<version>-windows-x64.zip` | Windows x64 | the desktop converter ([`apps/converter`](../../apps/converter)) |
| `bread-thumbnails-<version>-windows-x64.zip` | Windows x64 | the Explorer thumbnails ([`crates/bread-thumbnails`](../../crates/bread-thumbnails)), with `register.cmd` and `unregister.cmd` |

Each archive holds the program, a README for people who are not programmers (`README-*.txt`
here), `LICENSE` and the program's `NOTICE`, and Bread Converter also the font's license. Each
carries its program's own version, from its `Cargo.toml`. `SHA256SUMS.txt` lists every archive's
checksum.

The Windows programs link the C runtime statically, so they need no Visual C++ redistributable.
Every build maps the building machine's paths (the home folder, the cargo home, the toolchain,
the checkout) to short fixed names, and [`check-paths.sh`](check-paths.sh) fails the build if one
is still in a program. The Linux build runs on the oldest supported Ubuntu, so that it runs on
older systems too.

## Making a release

1. Bump the version in the `Cargo.toml` of each program that changed, and merge.
2. Tag `main` and push the tag:

   ```
   git tag tools-v0.1.0
   git push origin tools-v0.1.0
   ```

3. The workflow builds every archive and makes a **draft** release for the tag. Download the
   archives from it and try them, on a machine that never built them if possible.
4. Publish the draft on GitHub.

A pull request that changes the workflow or this folder builds everything as well, without making
a release: the archives are kept as the run's artifacts. The workflow can also be run by hand from
the Actions tab, with the same result.

## Building one by hand

```
cargo build --release -p bread-cli -p bread-converter -p bread-thumbnails
tools/release/package.sh windows-x64 target/release dist
```

`package.sh` takes the platform (`windows-x64`, `linux-x64` or `macos-arm64`), the release folder
and where to write. For a build to hand out, use the `RUSTFLAGS` of the workflow's build step.
