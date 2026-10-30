/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The `bread-format` crate for JavaScript: built to WebAssembly, so that the
//! browser reads `.bread` files with the same code, checks and all, as every
//! other program.
//!
//! The functions here are plain Rust and tested natively; [`abi`] puts them
//! behind a small C ABI that the JavaScript package calls.

use bread_format::{BreadReader, Chunk, ChunkTag, ReadBreadError, supported_version};

pub mod abi;

/// Why a file was refused: a reason code of `conformance/README.md`, and the
/// chunk it is about, when it is about one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: &'static str,
    pub chunk: Option<ChunkTag>,
}

/// The chunks of the file `bytes` that this build knows, in file order, each
/// checked and decompressed. Unknown chunks, official or private, are
/// skipped; a known chunk at a version this build does not read refuses the
/// file. Older supports and hollows chunks are kept, at their version, for
/// the reader to skip.
pub fn read_chunks(bytes: &[u8]) -> Result<Vec<Chunk>, Refusal> {
    let mut reader = BreadReader::new(bytes).map_err(refusal)?;
    let mut chunks = Vec::new();
    while let Some(chunk) = reader.next_chunk().map_err(refusal)? {
        let Some(supported) = supported_version(chunk.tag) else {
            continue;
        };
        if chunk.version == 0 || chunk.version > supported {
            return Err(Refusal {
                reason: "version",
                chunk: Some(chunk.tag),
            });
        }
        chunks.push(chunk);
    }
    Ok(chunks)
}

fn refusal(err: ReadBreadError) -> Refusal {
    let (reason, chunk) = match err {
        ReadBreadError::NotBread => ("not-bread", None),
        ReadBreadError::UnsupportedVersion(_) => ("version", None),
        // Reading from memory only fails by running out of bytes.
        ReadBreadError::Truncated | ReadBreadError::Io(_) => ("cut-short", None),
        ReadBreadError::Corrupt(tag) => ("crc", Some(tag)),
        ReadBreadError::BadPayload(tag) => ("bad-payload", Some(tag)),
    };
    Refusal { reason, chunk }
}
