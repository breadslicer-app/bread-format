/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

// The MSVC linker warns that COM's exports (DllGetClassObject and the rest)
// should be PRIVATE, which only a .def file can say. They are found by name,
// never imported, so the warning does not apply here.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        println!("cargo:rustc-cdylib-link-arg=/IGNORE:4104");
    }
}
