/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The name and comment a scene, a node or a mesh may carry.

use crate::bytes::{In, Out};

/// A name, and free text that came with it, such as an STL header.
///
/// Where it is optional, `None` (no metadata) is not the same as metadata
/// whose name and comment are both absent: the bytes differ.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub name: Option<String>,
    pub comment: Option<String>,
}

pub(crate) fn write_meta(out: &mut Out, meta: Option<&Meta>) {
    match meta {
        None => out.u8(0),
        Some(meta) => {
            out.u8(1);
            out.text(meta.name.as_deref());
            out.text(meta.comment.as_deref());
        }
    }
}

pub(crate) fn read_meta(input: &mut In<'_>) -> Option<Option<Meta>> {
    match input.u8()? {
        0 => Some(None),
        1 => Some(Some(Meta {
            name: input.text()?,
            comment: input.text()?,
        })),
        _ => None,
    }
}
