/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The thumbnail of a `.bread` file, read, decoded and fitted, in plain Rust.

use std::io::{Cursor, Read};

use bread_format::{BreadReader, THUMBNAIL_TAG, THUMBNAIL_VERSION};

/// The largest width or height a thumbnail may have (`SPEC.md`, 8.1).
pub const MAX_SIDE: u32 = 512;

/// The most stored or raw bytes a thumbnail chunk may have here. A PNG of
/// 512 × 512 pixels in RGBA is 1 MiB before compression, so a real thumbnail
/// stays far below; a chunk past this is damaged or not meant for us.
const MAX_CHUNK: u64 = 4 << 20;

/// The most memory the PNG decoder may take.
const MAX_DECODE: usize = 16 << 20;

/// The file header and the first chunk's header: 10 and 26 bytes.
const HEADS: usize = 10 + 26;

/// A picture in 8-bit RGBA, rows from the top, alpha not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The thumbnail of a `.bread` file: the PNG in its first chunk, or `None`.
///
/// Only the first chunk is read, where section 8.1 asks writers to put the
/// thumbnail. That chunk is checked like any other (header, CRC, one
/// Zstandard frame, raw length) by [`BreadReader`]. `None` when the input is
/// not a `.bread` file, its first chunk is not a thumbnail of version 1, or
/// the chunk is damaged or larger than a thumbnail can be.
pub fn read_thumbnail(mut input: impl Read) -> Option<Vec<u8>> {
    let mut bytes = vec![0; HEADS];
    input.read_exact(&mut bytes).ok()?;
    if bytes[10..14] != THUMBNAIL_TAG.0 {
        return None;
    }
    let raw_len = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
    let stored_len = u64::from_le_bytes(bytes[24..32].try_into().ok()?);
    if raw_len > MAX_CHUNK || stored_len > MAX_CHUNK {
        return None;
    }
    input.take(stored_len).read_to_end(&mut bytes).ok()?;
    // The reader checks the header and the chunk, and refuses one cut short.
    let mut reader = BreadReader::new(Cursor::new(bytes)).ok()?;
    let chunk = reader.next_chunk().ok()??;
    (chunk.version == THUMBNAIL_VERSION).then_some(chunk.payload)
}

/// Decodes a thumbnail's PNG into RGBA, or `None` when it is not a PNG this
/// decodes or is larger than [`MAX_SIDE`] either way. The size comes from the
/// PNG header and is checked before any pixel is decoded (`SPEC.md`, 8.2).
pub fn decode_png(png: &[u8]) -> Option<Picture> {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_limits(png::Limits { bytes: MAX_DECODE });
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    if frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let pixels = (width * height) as usize;
    let mut samples = buffer;
    samples.truncate(frame.line_size * height as usize);
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => samples,
        png::ColorType::Rgb => samples
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => samples
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => samples.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == pixels * 4).then_some(Picture {
        width,
        height,
        rgba,
    })
}

/// The picture scaled down to fit a square of `side` pixels, keeping its
/// shape; unchanged when it already fits. Each pixel is the average of the
/// pixels it covers, weighted by their alpha, so that transparent pixels do
/// not darken the edges they touch.
pub fn fit(picture: &Picture, side: u32) -> Picture {
    let (width, height) = (picture.width, picture.height);
    let longest = width.max(height);
    if side == 0 || longest <= side {
        return picture.clone();
    }
    let scaled = |n: u32| {
        ((u64::from(n) * u64::from(side) + u64::from(longest) / 2) / u64::from(longest)).max(1)
            as u32
    };
    let (new_width, new_height) = (scaled(width), scaled(height));
    let span = |i: u32, new: u32, old: u32| {
        let start = (u64::from(i) * u64::from(old) / u64::from(new)) as u32;
        let end = (u64::from(i + 1) * u64::from(old) / u64::from(new)) as u32;
        start..end.max(start + 1)
    };
    let mut rgba = Vec::with_capacity((new_width * new_height * 4) as usize);
    for y in 0..new_height {
        let rows = span(y, new_height, height);
        for x in 0..new_width {
            let columns = span(x, new_width, width);
            let (mut sum, mut alpha, mut count) = ([0u64; 3], 0u64, 0u64);
            for row in rows.clone() {
                for column in columns.clone() {
                    let at = ((row * width + column) * 4) as usize;
                    let pixel = &picture.rgba[at..at + 4];
                    let a = u64::from(pixel[3]);
                    for (total, &value) in sum.iter_mut().zip(pixel) {
                        *total += u64::from(value) * a;
                    }
                    alpha += a;
                    count += 1;
                }
            }
            if alpha == 0 {
                rgba.extend([0, 0, 0, 0]);
            } else {
                rgba.extend(sum.map(|total| ((total + alpha / 2) / alpha) as u8));
                rgba.push(((alpha + count / 2) / count) as u8);
            }
        }
    }
    Picture {
        width: new_width,
        height: new_height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bread_format::{BreadWriter, ChunkTag, SCENE_TAG};

    fn png(width: u32, height: u32, color: png::ColorType, samples: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(samples).unwrap();
        writer.finish().unwrap();
        out
    }

    fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        let samples = pixel.repeat((width * height) as usize);
        png(width, height, png::ColorType::Rgba, &samples)
    }

    fn bread(chunks: &[(ChunkTag, u16, &[u8])]) -> Vec<u8> {
        let mut writer = BreadWriter::new(Vec::new()).unwrap();
        for (tag, version, payload) in chunks {
            writer.chunk(*tag, *version, payload).unwrap();
        }
        writer.finish().unwrap()
    }

    #[test]
    fn reads_the_thumbnail_in_the_first_chunk() {
        let picture = solid(4, 2, [10, 20, 30, 255]);
        let file = bread(&[(THUMBNAIL_TAG, 1, &picture), (SCENE_TAG, 1, b"anything")]);
        assert_eq!(read_thumbnail(file.as_slice()), Some(picture));
    }

    #[test]
    fn a_thumbnail_after_another_chunk_is_not_read() {
        let picture = solid(4, 2, [10, 20, 30, 255]);
        let file = bread(&[(SCENE_TAG, 1, b"anything"), (THUMBNAIL_TAG, 1, &picture)]);
        assert_eq!(read_thumbnail(file.as_slice()), None);
    }

    #[test]
    fn a_damaged_or_newer_thumbnail_is_none() {
        let picture = solid(4, 2, [10, 20, 30, 255]);
        let mut damaged = bread(&[(THUMBNAIL_TAG, 1, &picture)]);
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        assert_eq!(read_thumbnail(damaged.as_slice()), None);

        let mut cut = bread(&[(THUMBNAIL_TAG, 1, &picture)]);
        cut.truncate(cut.len() - 3);
        assert_eq!(read_thumbnail(cut.as_slice()), None);

        let newer = bread(&[(THUMBNAIL_TAG, 2, &picture)]);
        assert_eq!(read_thumbnail(newer.as_slice()), None);
    }

    #[test]
    fn huge_lengths_are_refused_before_reading() {
        let mut file = bread(&[(THUMBNAIL_TAG, 1, &solid(1, 1, [0; 4]))]);
        file[24..32].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(read_thumbnail(file.as_slice()), None);
    }

    #[test]
    fn not_a_bread_file_is_none() {
        assert_eq!(read_thumbnail(&b"solid cube\nendsolid\n"[..]), None);
        assert_eq!(read_thumbnail(&b""[..]), None);
        let mut wrong_magic = bread(&[(THUMBNAIL_TAG, 1, &solid(1, 1, [0; 4]))]);
        wrong_magic[0] = b'b';
        assert_eq!(read_thumbnail(wrong_magic.as_slice()), None);
    }

    #[test]
    fn decodes_every_colour_type_to_rgba() {
        let rgb = png(2, 1, png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(decode_png(&rgb).unwrap().rgba, [1, 2, 3, 255, 4, 5, 6, 255]);
        let gray = png(2, 1, png::ColorType::Grayscale, &[7, 8]);
        assert_eq!(
            decode_png(&gray).unwrap().rgba,
            [7, 7, 7, 255, 8, 8, 8, 255]
        );
        let gray_alpha = png(1, 1, png::ColorType::GrayscaleAlpha, &[9, 100]);
        assert_eq!(decode_png(&gray_alpha).unwrap().rgba, [9, 9, 9, 100]);
        let rgba = decode_png(&solid(3, 2, [1, 2, 3, 4])).unwrap();
        assert_eq!((rgba.width, rgba.height), (3, 2));
        assert_eq!(rgba.rgba, [1, 2, 3, 4].repeat(6));
    }

    #[test]
    fn a_picture_larger_than_512_is_not_decoded() {
        assert!(decode_png(&solid(512, 1, [0; 4])).is_some());
        assert_eq!(decode_png(&solid(513, 1, [0; 4])), None);
        assert_eq!(decode_png(b"not a png"), None);
    }

    #[test]
    fn fit_keeps_the_shape_and_leaves_small_pictures_alone() {
        let picture = decode_png(&solid(512, 256, [50, 60, 70, 255])).unwrap();
        let fitted = fit(&picture, 256);
        assert_eq!((fitted.width, fitted.height), (256, 128));
        assert!(
            fitted
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [50, 60, 70, 255])
        );
        assert_eq!(fit(&picture, 1024), picture);
        let tiny = fit(&decode_png(&solid(500, 3, [0; 4])).unwrap(), 16);
        assert_eq!((tiny.width, tiny.height), (16, 1));
    }

    #[test]
    fn fit_does_not_darken_edges_with_transparent_pixels() {
        // White and transparent black, averaged: white at half alpha, not grey.
        let picture = Picture {
            width: 2,
            height: 1,
            rgba: vec![255, 255, 255, 255, 0, 0, 0, 0],
        };
        assert_eq!(fit(&picture, 1).rgba, [255, 255, 255, 128]);
    }
}
