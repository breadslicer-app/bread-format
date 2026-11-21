/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Reads and writes `.bread` files: triangle meshes and the scene that places
//! them, stored small and without losing anything. The format is described in
//! [`SPEC.md`](https://github.com/breadslicer-app/bread-format/blob/main/SPEC.md),
//! in the repository.
//!
//! [`read_file`] and [`write_file`] read and write a whole file as a
//! [`BreadFile`]: the scene, its meshes, and the documents programs keep
//! about them, such as editable supports.
//!
//! A `.bread` file is a short header followed by chunks. Each chunk has a
//! four-byte tag, its own version, and a payload compressed with zstd and
//! guarded by a CRC-32. [`container`] reads and writes chunks without knowing
//! what they hold; [`scene`] and [`mesh`] are the codecs of the scene chunk
//! and of the mesh chunks it places, [`app`] carries the documents of the
//! application chunks, [`preview`] the meshes that show supports to readers
//! that do not rebuild them, [`surface`] the paint on the faces of models, and
//! [`role`] which leaves are supports or a raft rather than models.
//! [`metadata`] says who made a file and how it may be used, and gives its
//! parts lasting ids.
//! [`stl`] reads STL files into meshes, to be written as `.bread`.

pub mod app;
mod bytes;
pub mod container;
pub mod file;
pub mod mesh;
pub mod mesh_v2;
mod meta;
pub mod metadata;
mod parallel;
pub mod preview;
pub mod role;
pub mod scene;
pub mod stl;
pub mod surface;

#[cfg(test)]
mod tests_app;
#[cfg(test)]
mod tests_container;
#[cfg(test)]
mod tests_file;
#[cfg(test)]
mod tests_mesh;
#[cfg(test)]
mod tests_mesh_v2;
#[cfg(test)]
mod tests_metadata;
#[cfg(test)]
mod tests_preview;
#[cfg(test)]
mod tests_role;
#[cfg(test)]
mod tests_scene;
#[cfg(test)]
mod tests_stl;
#[cfg(test)]
mod tests_surface;

pub use app::{
    DecodeDocumentsError, EncodeDocumentsError, HOLLOWS_TAG, HOLLOWS_VERSION, HollowEntry,
    SETTINGS_TAG, SETTINGS_VERSION, SUPPORTS_TAG, SUPPORTS_VERSION, SupportEntry, decode_hollows,
    decode_supports, encode_hollows, encode_supports,
};
pub use container::{BreadReader, BreadWriter, Chunk, ChunkTag, ReadBreadError};
pub use file::{
    BreadFile, ReadFileError, THUMBNAIL_TAG, THUMBNAIL_VERSION, WriteFileError, read_file,
    supported_version, write_file,
};
pub use mesh::{
    DecodeMeshError, EncodeMeshError, MESH_TAG, MESH_VERSION, Mesh, decode_mesh, encode_mesh,
};
pub use mesh_v2::{
    Connectivity, MeshCoding, VertexCoding, decode_mesh_v2, encode_mesh_v2, encode_mesh_v2_with,
};
pub use meta::Meta;
pub use metadata::{
    EncodeMetadataError, METADATA_TAG, METADATA_VERSION, MetadataEntry, TEXT_KEYS, UUID_KEY,
    decode_metadata, encode_metadata,
};
pub use preview::{
    DecodePreviewsError, EncodePreviewsError, SUPPORT_PREVIEW_TAG, SUPPORT_PREVIEW_VERSION,
    SupportPreview, decode_previews, encode_previews,
};
pub use role::{
    DecodeRolesError, EncodeRolesError, ROLE_RAFT, ROLE_SUPPORTS, ROLE_TAG, ROLE_VERSION,
    RoleEntry, decode_roles, encode_roles,
};
pub use scene::{
    DecodeSceneError, EncodeSceneError, Node, NodeKind, SCENE_TAG, SCENE_VERSION, Scene, Transform,
    decode_scene, encode_scene,
};
pub use stl::{ReadStlError, read_stl};
pub use surface::{
    DecodeSurfaceError, EncodeSurfaceError, SURFACE_TAG, SURFACE_VERSION, SurfaceEntry,
    decode_surface, encode_surface,
};
