# bread-wasm

The `bread-format` crate built to WebAssembly, for the JavaScript package. One implementation:
the browser reads `.bread` files with the same code and the same checks as every other program.

`src/lib.rs` holds plain Rust functions, tested natively with the rest of the workspace.
`src/abi.rs` puts them behind a small C ABI: JavaScript copies a file into the module's memory,
calls a function, and copies the results out. Because it takes raw pointers from JavaScript, it
is the only module in the repository allowed to use `unsafe` (its tests, which call it the same
way, are too). `bread-format` itself still forbids it.

## Building

The module needs a C compiler that targets WebAssembly, because meshoptimizer and zstd are C and
C++: clang, for example from the [WebAssembly SDK](https://github.com/WebAssembly/wasi-sdk).
The `meshopt` crate builds meshoptimizer against its own small C headers, which lack a few math
functions, so every C file gets [`wasm/meshopt-math.h`](wasm/meshopt-math.h) forced in.

```sh
rustup target add wasm32-unknown-unknown
export CC_wasm32_unknown_unknown=/path/to/clang
export CXX_wasm32_unknown_unknown=/path/to/clang++
export PATH="$PATH:/path/to/the/folder/of/llvm-ar"
export CFLAGS_wasm32_unknown_unknown="-include $PWD/crates/bread-wasm/wasm/meshopt-math.h"
export CXXFLAGS_wasm32_unknown_unknown="$CFLAGS_wasm32_unknown_unknown"
cargo build -p bread-wasm --profile wasm --target wasm32-unknown-unknown
```

The module is `target/wasm32-unknown-unknown/wasm/bread_wasm.wasm`. It imports nothing.

The third-party notices for meshoptimizer and zstd are in [NOTICE](NOTICE); keep them with the
module wherever it goes.
