/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The C ABI the JavaScript package calls. JavaScript copies a file into
//! memory from [`bread_alloc`], calls [`bread_read_chunks`], frees the input,
//! and copies out what it needs through the accessors. The result is kept
//! until the next read, or [`bread_release`].
//!
//! The accessors return 0, or a null pointer, for a chunk index out of range
//! or when the last read was refused, so a mistake in JavaScript cannot read
//! past the results. Memory can grow on any call: JavaScript makes its views
//! of it again after each one.

#![allow(unsafe_code)]

use std::cell::RefCell;

use bread_format::Chunk;

use crate::{Refusal, read_chunks};

thread_local! {
    static RESULT: RefCell<Result<Vec<Chunk>, Refusal>> = const { RefCell::new(Ok(Vec::new())) };
}

fn with_chunk<T>(index: usize, none: T, f: impl FnOnce(&Chunk) -> T) -> T {
    RESULT.with_borrow(|result| match result {
        Ok(chunks) => chunks.get(index).map_or(none, f),
        Err(_) => none,
    })
}

fn with_refusal<T>(none: T, f: impl FnOnce(&Refusal) -> T) -> T {
    RESULT.with_borrow(|result| result.as_ref().err().map_or(none, f))
}

/// Reserves `len` bytes for JavaScript to fill, and returns where they are.
#[unsafe(no_mangle)]
pub extern "C" fn bread_alloc(len: usize) -> *mut u8 {
    let mut bytes = Vec::<u8>::with_capacity(len);
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

/// Frees what [`bread_alloc`] reserved.
///
/// # Safety
///
/// `ptr` and `len` come from one call to [`bread_alloc`], not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_free(ptr: *mut u8, len: usize) {
    // SAFETY: the caller hands back an allocation of `len` bytes from
    // `bread_alloc`, which made it as a `Vec<u8>` of that capacity.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len) });
}

/// Reads the chunks of the `len` bytes at `ptr`, as [`read_chunks`] does.
/// Returns 1 when the file was read and 0 when it was refused.
///
/// # Safety
///
/// `ptr` points to `len` initialized bytes, such as those from
/// [`bread_alloc`] once JavaScript has filled them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bread_read_chunks(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller promises `len` readable bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let result = read_chunks(bytes);
    let read = u32::from(result.is_ok());
    RESULT.set(result);
    read
}

/// Drops the result of the last read.
#[unsafe(no_mangle)]
pub extern "C" fn bread_release() {
    RESULT.set(Ok(Vec::new()));
}

/// How many chunks the last read returned.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_count() -> usize {
    RESULT.with_borrow(|result| result.as_ref().map_or(0, Vec::len))
}

/// The tag of chunk `index`: its four bytes read as a little-endian `u32`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_tag(index: usize) -> u32 {
    with_chunk(index, 0, |chunk| u32::from_le_bytes(chunk.tag.0))
}

/// The version of chunk `index`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_version(index: usize) -> u32 {
    with_chunk(index, 0, |chunk| u32::from(chunk.version))
}

/// Where the payload of chunk `index` is.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_payload(index: usize) -> *const u8 {
    with_chunk(index, std::ptr::null(), |chunk| chunk.payload.as_ptr())
}

/// The length of the payload of chunk `index`.
#[unsafe(no_mangle)]
pub extern "C" fn bread_chunk_payload_len(index: usize) -> usize {
    with_chunk(index, 0, |chunk| chunk.payload.len())
}

/// Where the reason code of the last refusal is, in UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_reason() -> *const u8 {
    with_refusal(std::ptr::null(), |refusal| refusal.reason.as_ptr())
}

/// The length of the reason code of the last refusal.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_reason_len() -> usize {
    with_refusal(0, |refusal| refusal.reason.len())
}

/// The tag of the chunk the last refusal is about, as [`bread_chunk_tag`]
/// gives it, or 0 when it is about the whole file.
#[unsafe(no_mangle)]
pub extern "C" fn bread_refusal_chunk() -> u32 {
    with_refusal(0, |refusal| {
        refusal.chunk.map_or(0, |tag| u32::from_le_bytes(tag.0))
    })
}
