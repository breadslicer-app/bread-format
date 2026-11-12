/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! C++'s `operator new` and `operator delete`, for WebAssembly.
//!
//! meshoptimizer is C++, and the functions that encode meshes take their
//! scratch memory through `operator new` and give it back through `operator
//! delete`. The wasm32-unknown-unknown target has no C++ library to supply
//! them, so they are made here on Rust's allocator. The decoders never call
//! them, which is why a module that only reads did not need them.
//!
//! `operator delete` is not told the size, so each block keeps it in a
//! header in front of what it hands out.

#![allow(unsafe_code)]

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};

/// The header in front of each block: its size, padded so what follows is
/// aligned for anything C++ puts there.
const HEADER: usize = 16;

fn layout(size: usize) -> Layout {
    size.checked_add(HEADER)
        .and_then(|total| Layout::from_size_align(total, HEADER).ok())
        .unwrap_or_else(|| handle_alloc_error(Layout::new::<u8>()))
}

/// `operator new(size_t)`: never returns null; running out of memory aborts,
/// as an uncaught `std::bad_alloc` would.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn _Znwm(size: usize) -> *mut u8 {
    let layout = layout(size);
    // SAFETY: the layout is at least HEADER bytes, never zero.
    let block = unsafe { alloc(layout) };
    if block.is_null() {
        handle_alloc_error(layout);
    }
    // SAFETY: the block holds HEADER bytes, aligned for a usize, before
    // what it hands out.
    unsafe {
        block.cast::<usize>().write(size);
        block.add(HEADER)
    }
}

/// `operator new[](size_t)`, the same.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn _Znam(size: usize) -> *mut u8 {
    _Znwm(size)
}

/// `operator delete(void*)`: gives back a block from [`_Znwm`]; null is
/// allowed and does nothing.
///
/// # Safety
///
/// `ptr` is null or came from [`_Znwm`] or [`_Znam`], not given back yet.
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn _ZdlPv(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` is HEADER bytes past the start of a block from `_Znwm`,
    // which wrote the size there.
    unsafe {
        let block = ptr.sub(HEADER);
        let size = block.cast::<usize>().read();
        dealloc(block, layout(size));
    }
}

/// `operator delete[](void*)`, the same.
///
/// # Safety
///
/// As for [`_ZdlPv`].
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn _ZdaPv(ptr: *mut u8) {
    // SAFETY: as the caller promises.
    unsafe { _ZdlPv(ptr) }
}

/// `operator delete(void*, size_t)`, which C++14 may call instead.
///
/// # Safety
///
/// As for [`_ZdlPv`].
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn _ZdlPvm(ptr: *mut u8, _size: usize) {
    // SAFETY: as the caller promises.
    unsafe { _ZdlPv(ptr) }
}

/// `operator delete[](void*, size_t)`.
///
/// # Safety
///
/// As for [`_ZdlPv`].
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn _ZdaPvm(ptr: *mut u8, _size: usize) {
    // SAFETY: as the caller promises.
    unsafe { _ZdlPv(ptr) }
}
