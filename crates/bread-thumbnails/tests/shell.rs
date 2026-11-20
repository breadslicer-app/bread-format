/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The provider through its COM interfaces, as Explorer calls it, in this
//! process.

#![cfg(windows)]
#![allow(unsafe_code)]

use bread_format::{BreadWriter, SCENE_TAG, THUMBNAIL_TAG};
use bread_thumbnails::ThumbnailProvider;
use windows::Win32::Graphics::Gdi::{DIBSECTION, DeleteObject, GetObjectW, HBITMAP};
use windows::Win32::UI::Shell::PropertiesSystem::IInitializeWithStream;
use windows::Win32::UI::Shell::{IThumbnailProvider, SHCreateMemStream, WTS_ALPHATYPE, WTSAT_ARGB};
use windows_core::Interface;

/// A `.bread` file whose thumbnail is `width` × `height` pixels: red in the
/// top left quarter, transparent elsewhere.
fn file(width: u32, height: u32, thumbnail_first: bool) -> Vec<u8> {
    let mut samples = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let red = x < width / 2 && y < height / 2;
            samples.extend(if red { [200, 10, 20, 255] } else { [0; 4] });
        }
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&samples).unwrap();
    writer.finish().unwrap();

    let mut bread = BreadWriter::new(Vec::new()).unwrap();
    if thumbnail_first {
        bread.chunk(THUMBNAIL_TAG, 1, &png).unwrap();
    }
    bread.chunk(SCENE_TAG, 1, b"not read").unwrap();
    if !thumbnail_first {
        bread.chunk(THUMBNAIL_TAG, 1, &png).unwrap();
    }
    bread.finish().unwrap()
}

/// What the provider answers for `file` at `cx`: the bitmap's size and its
/// pixels in BGRA, or the error.
fn thumbnail(file: &[u8], cx: u32) -> windows_core::Result<(i32, i32, Vec<u8>)> {
    let provider: IInitializeWithStream = ThumbnailProvider::new().into();
    // SAFETY: SHCreateMemStream copies the bytes.
    let stream = unsafe { SHCreateMemStream(Some(file)) }.unwrap();
    // SAFETY: a live stream, as Explorer passes one.
    unsafe { provider.Initialize(&stream, 0) }?;
    let provider: IThumbnailProvider = provider.cast()?;
    let (mut bitmap, mut alpha) = (HBITMAP::default(), WTS_ALPHATYPE::default());
    // SAFETY: valid places for the two results.
    unsafe { provider.GetThumbnail(cx, &mut bitmap, &mut alpha) }?;
    assert_eq!(alpha, WTSAT_ARGB);
    let mut section = DIBSECTION::default();
    // SAFETY: `bitmap` is the provider's DIB section, and `section` has room
    // for its description.
    let size = unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<DIBSECTION>() as i32,
            Some((&raw mut section).cast()),
        )
    };
    assert_eq!(size as usize, size_of::<DIBSECTION>());
    let header = section.dsBmih;
    assert_eq!((header.biBitCount, header.biCompression), (32, 0));
    // GetObjectW reports the height as positive, top-down or not.
    let (width, height) = (header.biWidth, header.biHeight.abs());
    let len = (width * height * 4) as usize;
    // SAFETY: the section's bits hold width × height pixels of 4 bytes.
    let pixels = unsafe { std::slice::from_raw_parts(section.dsBm.bmBits.cast::<u8>(), len) };
    let pixels = pixels.to_vec();
    // SAFETY: the caller owns the bitmap the provider returns.
    let _ = unsafe { DeleteObject(bitmap.into()) };
    Ok((width, height, pixels))
}

#[test]
fn draws_the_thumbnail_as_explorer_asks() {
    let (width, height, pixels) = thumbnail(&file(256, 256, true), 256).unwrap();
    assert_eq!((width, height), (256, 256));
    // BGRA, rows from the top: red top left, transparent bottom left.
    assert_eq!(&pixels[..4], [20, 10, 200, 255]);
    let bottom_left = (255 * 256 * 4) as usize;
    assert_eq!(&pixels[bottom_left..bottom_left + 4], [0, 0, 0, 0]);
    assert_eq!(&pixels[pixels.len() - 4..], [0, 0, 0, 0]);
}

#[test]
fn scales_down_to_the_size_asked() {
    let (width, height, pixels) = thumbnail(&file(256, 128, true), 96).unwrap();
    assert_eq!((width, height), (96, 48));
    assert_eq!(&pixels[..4], [20, 10, 200, 255]);
}

#[test]
fn no_thumbnail_in_the_first_chunk_is_an_error() {
    assert!(thumbnail(&file(16, 16, false), 96).is_err());
    assert!(thumbnail(b"solid cube\nendsolid\n", 96).is_err());
}

#[test]
fn a_provider_is_initialized_once() {
    let provider: IInitializeWithStream = ThumbnailProvider::new().into();
    // SAFETY: SHCreateMemStream copies the bytes.
    let stream = unsafe { SHCreateMemStream(Some(b"x")) }.unwrap();
    // SAFETY: a live stream.
    unsafe { provider.Initialize(&stream, 0) }.unwrap();
    // SAFETY: as above.
    assert!(unsafe { provider.Initialize(&stream, 0) }.is_err());
}
