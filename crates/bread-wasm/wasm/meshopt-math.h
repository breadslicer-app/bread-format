/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

/*
 * Forced into every C and C++ file when building for wasm32-unknown-unknown.
 * The meshopt crate builds meshoptimizer for that target against its own
 * small set of C headers, and those leave out some math functions that newer
 * meshoptimizer sources call. None of them is in the mesh codecs: mapping
 * them to compiler built-ins lets the rest of the library compile, and the
 * linker drops it.
 */
#if defined(__wasm__)
#define ceilf __builtin_ceilf
#define cosf __builtin_cosf
#define floorf __builtin_floorf
#define frexpf __builtin_frexpf
#define ldexpf __builtin_ldexpf
#define log2f __builtin_log2f
#endif
