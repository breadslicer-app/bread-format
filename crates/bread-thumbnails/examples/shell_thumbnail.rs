/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Asks Windows for a file's thumbnail the way Explorer does, through the
//! shell and whatever provider is registered, and writes it as a PNG:
//!
//! ```text
//! cargo run -p bread-thumbnails --example shell_thumbnail -- model.bread out.png [size]
//! ```
//!
//! It fails when the shell has no thumbnail for the file, for example because
//! no provider is registered for its type.

#![allow(unsafe_code)]

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW,
    };
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
    };
    use windows_core::HSTRING;

    let args: Vec<String> = std::env::args().collect();
    let [_, input, output, rest @ ..] = args.as_slice() else {
        return Err("usage: shell_thumbnail <file> <out.png> [size]".into());
    };
    let size: i32 = rest.first().map_or(Ok(256), |s| s.parse())?;
    let input = std::path::absolute(input)?;

    // SAFETY: once, on this thread, before any other COM call.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
    // SAFETY: a path and no bind context.
    let item: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(input.as_os_str()), None) }?;
    let wanted = SIZE { cx: size, cy: size };
    // SAFETY: a live item.
    let bitmap = unsafe { item.GetImage(wanted, SIIGBF_THUMBNAILONLY) }?;

    let mut description = BITMAP::default();
    // SAFETY: `description` has room for a BITMAP.
    unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&raw mut description).cast()),
        )
    };
    let (width, height) = (description.bmWidth, description.bmHeight);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    // SAFETY: a memory DC for GetDIBits, deleted below.
    let dc = unsafe { CreateCompatibleDC(None) };
    // SAFETY: `bgra` holds the height × width pixels of 4 bytes `info` asks for.
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: both were made above and are not used after this.
    unsafe {
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bitmap.into());
    }
    if rows != height {
        return Err("could not read the thumbnail's pixels".into());
    }

    let rgba: Vec<u8> = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    let mut encoder =
        png::Encoder::new(std::fs::File::create(output)?, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    println!("{width} x {height} -> {output}");
    Ok(())
}
