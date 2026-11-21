/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Thumbnails of `.bread` files in Windows Explorer.
//!
//! On Windows this crate builds `bread_thumbnails.dll`, a shell thumbnail
//! provider (`IThumbnailProvider`). Once it is registered for `.bread`,
//! Explorer shows the picture in a file's thumbnail chunk (`THMB`, section 8
//! of `SPEC.md`), and its usual icon for a file that has none. The README says
//! how to register it.
//!
//! [`read_thumbnail`] reads the picture the way section 8.1 lets a program
//! that only wants it: from the first chunk, and nothing after. A file that
//! does not start with its thumbnail has none here, so Explorer never reads or
//! decompresses a whole model to draw an icon. [`decode_png`] decodes the
//! picture once its header says it is no larger than 512 × 512 (section 8.2),
//! and [`fit`] scales it to the size Explorer asks for. These three build on
//! every platform and are tested there; the COM server around them is
//! Windows only.

mod thumbnail;

pub use thumbnail::{MAX_SIDE, Picture, decode_png, fit, read_thumbnail};

#[cfg(windows)]
#[allow(unsafe_code)]
mod shell;

#[cfg(windows)]
pub use shell::{CLSID, ThumbnailProvider};
