/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Reads and writes `.bread` files: triangle meshes and the scene that places
//! them, stored small and without losing anything. The format is described in
//! `SPEC.md` at the root of the repository.
//!
//! [`read_file`] and [`write_file`] read and write a whole file as a
//! [`BreadFile`]: the scene, its meshes, and the documents programs keep
//! about them, such as editable supports.
//!
//! A `.bread` file is a short header followed by chunks. Each chunk has a
//! four-byte tag, its own version, and a payload compressed with zstd and
//! guarded by a CRC-32. [`container`] reads and writes chunks without knowing
//! what they hold; [`scene`] and [`mesh`] are the codecs of the scene chunk
//! and of the mesh chunks it places, and [`app`] carries the documents of
//! the application chunks.

pub mod app;
mod bytes;
pub mod container;
pub mod file;
pub mod mesh;
mod meta;
pub mod scene;

#[cfg(test)]
mod tests_app;
#[cfg(test)]
mod tests_container;
#[cfg(test)]
mod tests_file;
#[cfg(test)]
mod tests_mesh;
#[cfg(test)]
mod tests_scene;

pub use app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, HOLLOWS_VERSION, HollowEntry,
    SETTINGS_TAG, SETTINGS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, SupportEntry, decode_hollows,
    decode_supports, encode_hollows, encode_supports,
};
pub use container::{BreadReader, BreadWriter, Chunk, ChunkTag, ReadBreadError};
pub use file::{
    BreadFile, ReadFileError, THUMBNAIL_TAG, THUMBNAIL_VERSION, WriteFileError, read_file,
    write_file,
};
pub use mesh::{
    DecodeMeshError, EncodeMeshError, MESH_TAG, MESH_VERSION, Mesh, decode_mesh, encode_mesh,
};
pub use meta::Meta;
pub use scene::{
    DecodeSceneError, EncodeSceneError, Node, NodeKind, SCENE_TAG, SCENE_VERSION, Scene, Transform,
    decode_scene, encode_scene,
};
