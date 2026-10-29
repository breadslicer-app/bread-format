/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Reads and writes `.bread` files: triangle meshes and the scene that places
//! them, stored small and without losing anything. The format is described in
//! `SPEC.md` at the root of the repository.
//!
//! A `.bread` file is a short header followed by chunks. Each chunk has a
//! four-byte tag, its own version, and a payload compressed with zstd and
//! guarded by a CRC-32. [`container`] reads and writes chunks without knowing
//! what they hold.

#[expect(dead_code, reason = "the chunk codecs that use it arrive next")]
mod bytes;
pub mod container;

#[cfg(test)]
mod tests_container;

pub use container::{BreadReader, BreadWriter, Chunk, ChunkTag, ReadBreadError};
