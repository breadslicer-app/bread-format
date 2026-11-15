/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Compiles the window, with its Spanish texts, and gives the Windows
//! program its icon.

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("i18n");
    slint_build::compile_with_config("ui/app.slint", config).expect("the window compiles");
    println!("cargo:rerun-if-changed=i18n");

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.compile().expect("the icon is embedded");
    }
}
