/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The other ways to store a mesh that `--formats` measures `.bread`
//! against. Each one is written so that it loses nothing: OBJ and 3MF give
//! every coordinate as the shortest decimal that reads back as the same
//! `f32`, and the compressed STLs hold the STL's own bytes. Each is read back
//! into corners, so that what it keeps is checked, not assumed.

use std::fmt::Write as _;
use std::io::{Cursor, Read as _, Write as _};

use bread_format::{Mesh, read_stl};

/// Corners, as the bits of their coordinates, three per triangle in order.
pub(crate) type Corners = Vec<[u32; 3]>;

/// One of the ways measured.
#[derive(Clone, Copy)]
pub(crate) struct Format {
    /// Its key in the results.
    pub(crate) key: &'static str,
    /// What it is, for the results' header.
    pub(crate) what: &'static str,
    write: fn(&Mesh, &[u8], &Options) -> Vec<u8>,
    read: fn(&[u8]) -> Result<Corners, String>,
}

/// What the compressors are given.
pub(crate) struct Options {
    /// Threads for Zstandard, as `zstd -T`.
    pub(crate) zstd_threads: u32,
}

impl Format {
    pub(crate) fn write(&self, mesh: &Mesh, stl: &[u8], options: &Options) -> Vec<u8> {
        (self.write)(mesh, stl, options)
    }

    pub(crate) fn read(&self, bytes: &[u8]) -> Result<Corners, String> {
        (self.read)(bytes)
    }
}

/// Every format measured, in the order of the results.
pub(crate) const FORMATS: [Format; 8] = [
    Format {
        key: "stl",
        what: "binary STL, written from the mesh with its facet normals",
        write: |mesh, _, _| write_stl(mesh),
        read: read_stl_corners,
    },
    Format {
        key: "stl_zip_9",
        what: "the binary STL in a zip, Deflate at level 9",
        write: |_, stl, _| zip_one("model.stl", stl, 9),
        read: |bytes| read_stl_corners(&unzip(bytes, "model.stl")?),
    },
    Format {
        key: "stl_zstd_3",
        what: "the binary STL with Zstandard at level 3, its default",
        write: |_, stl, options| zstd(stl, 3, options),
        read: |bytes| read_stl_corners(&unzstd(bytes)?),
    },
    Format {
        key: "stl_zstd_19",
        what: "the binary STL with Zstandard at level 19",
        write: |_, stl, options| zstd(stl, 19, options),
        read: |bytes| read_stl_corners(&unzstd(bytes)?),
    },
    Format {
        key: "obj",
        what: "OBJ, every coordinate as the shortest decimal that reads back as the same f32",
        write: |mesh, _, _| write_obj(mesh).into_bytes(),
        read: |bytes| read_obj(bytes),
    },
    Format {
        key: "obj_gzip_9",
        what: "that OBJ with gzip at level 9",
        write: |mesh, _, _| gzip(write_obj(mesh).as_bytes()),
        read: |bytes| read_obj(&gunzip(bytes)?),
    },
    Format {
        key: "obj_zstd_19",
        what: "that OBJ with Zstandard at level 19",
        write: |mesh, _, options| zstd(write_obj(mesh).as_bytes(), 19, options),
        read: |bytes| read_obj(&unzstd(bytes)?),
    },
    Format {
        key: "3mf",
        what: "3MF, its model in a zip at Deflate's default level 6, the same decimals as the OBJ",
        write: |mesh, _, _| write_3mf(mesh),
        read: read_3mf,
    },
];

/// The corners of a mesh, as the bits of their coordinates, three per
/// triangle in order.
pub(crate) fn mesh_corners(mesh: &Mesh) -> Result<Corners, String> {
    mesh.triangles
        .as_flattened()
        .iter()
        .map(|&v| {
            mesh.vertices
                .get(v as usize)
                .map(|p| p.map(f32::to_bits))
                .ok_or_else(|| format!("no vertex {v}"))
        })
        .collect()
}

fn read_stl_corners(bytes: &[u8]) -> Result<Corners, String> {
    mesh_corners(&read_stl(bytes).map_err(|e| e.to_string())?)
}

/// A binary STL of the mesh, each facet with its normal, as an STL writer
/// would make it.
fn write_stl(mesh: &Mesh) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + 50 * mesh.triangles.len());
    out.extend_from_slice(&[0; 80]);
    out.extend_from_slice(&(mesh.triangles.len() as u32).to_le_bytes());
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|v| mesh.vertices[v as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if length > 0.0 {
            n.map(|x| x / length)
        } else {
            [0.0; 3]
        };
        for p in [n, a, b, c] {
            for x in p {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

/// The mesh as OBJ text. Rust prints an `f32` as the shortest decimal that
/// parses back to the same value.
fn write_obj(mesh: &Mesh) -> String {
    let mut out = String::with_capacity(32 * mesh.vertices.len() + 24 * mesh.triangles.len());
    for [x, y, z] in &mesh.vertices {
        let _ = writeln!(out, "v {x} {y} {z}");
    }
    for [a, b, c] in &mesh.triangles {
        let _ = writeln!(out, "f {} {} {}", a + 1, b + 1, c + 1);
    }
    out
}

fn read_obj(bytes: &[u8]) -> Result<Corners, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let mut vertices: Vec<[u32; 3]> = Vec::new();
    let mut corners = Vec::new();
    for line in text.lines() {
        let mut words = line.split_ascii_whitespace();
        match words.next() {
            Some("v") => {
                let mut p = [0; 3];
                for x in &mut p {
                    let word = words
                        .next()
                        .ok_or("a vertex with fewer than 3 coordinates")?;
                    *x = word.parse::<f32>().map_err(|e| e.to_string())?.to_bits();
                }
                vertices.push(p);
            }
            Some("f") => {
                for word in words {
                    let index: usize = word.parse().map_err(|_| format!("bad index {word}"))?;
                    let p = index
                        .checked_sub(1)
                        .and_then(|i| vertices.get(i))
                        .ok_or_else(|| format!("no vertex {index}"))?;
                    corners.push(*p);
                }
            }
            _ => {}
        }
    }
    Ok(corners)
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;
const MODEL: &str = "3D/3dmodel.model";

/// The mesh as a 3MF package: one object, built once.
fn write_3mf(mesh: &Mesh) -> Vec<u8> {
    let mut xml = String::with_capacity(48 * mesh.vertices.len() + 40 * mesh.triangles.len());
    xml.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\"><resources>\
         <object id=\"1\" type=\"model\"><mesh><vertices>",
    );
    for [x, y, z] in &mesh.vertices {
        let _ = write!(xml, "<vertex x=\"{x}\" y=\"{y}\" z=\"{z}\"/>");
    }
    xml.push_str("</vertices><triangles>");
    for [a, b, c] in &mesh.triangles {
        let _ = write!(xml, "<triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>");
    }
    xml.push_str(
        "</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>",
    );
    zip_files(
        &[
            ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
            ("_rels/.rels", RELS.as_bytes()),
            (MODEL, xml.as_bytes()),
        ],
        6,
    )
}

fn read_3mf(bytes: &[u8]) -> Result<Corners, String> {
    let xml = unzip(bytes, MODEL)?;
    let xml = std::str::from_utf8(&xml).map_err(|e| e.to_string())?;
    let mut vertices: Vec<[u32; 3]> = Vec::new();
    let mut corners = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let end = rest.find('>').ok_or("an element that does not end")?;
        let element = &rest[..end];
        if let Some(attributes) = element.strip_prefix("vertex ") {
            let mut p = [0; 3];
            for (x, name) in p.iter_mut().zip(["x", "y", "z"]) {
                *x = attribute(attributes, name)?
                    .parse::<f32>()
                    .map_err(|e| e.to_string())?
                    .to_bits();
            }
            vertices.push(p);
        } else if let Some(attributes) = element.strip_prefix("triangle ") {
            for name in ["v1", "v2", "v3"] {
                let index: usize = attribute(attributes, name)?
                    .parse()
                    .map_err(|_| "a bad index".to_owned())?;
                corners.push(
                    *vertices
                        .get(index)
                        .ok_or_else(|| format!("no vertex {index}"))?,
                );
            }
        }
        rest = &rest[end..];
    }
    Ok(corners)
}

fn attribute<'a>(attributes: &'a str, name: &str) -> Result<&'a str, String> {
    let key = format!("{name}=\"");
    let start = if attributes.starts_with(&key) {
        key.len()
    } else {
        attributes
            .find(&format!(" {key}"))
            .ok_or_else(|| format!("no {name}"))?
            + key.len()
            + 1
    };
    let value = &attributes[start..];
    Ok(&value[..value.find('"').ok_or("an attribute that does not end")?])
}

fn zip_one(name: &str, bytes: &[u8], level: i64) -> Vec<u8> {
    zip_files(&[(name, bytes)], level)
}

fn zip_files(files: &[(&str, &[u8])], level: i64) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(level))
        .large_file(true);
    for (name, bytes) in files {
        zip.start_file(*name, options).expect("a zip in memory");
        zip.write_all(bytes).expect("a zip in memory");
    }
    zip.finish().expect("a zip in memory").into_inner()
}

fn unzip(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut file = zip.by_name(name).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
    file.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn zstd(bytes: &[u8], level: i32, options: &Options) -> Vec<u8> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), level).expect("a Zstandard encoder");
    encoder
        .multithread(options.zstd_threads)
        .expect("Zstandard threads");
    encoder
        .include_contentsize(true)
        .expect("Zstandard content size");
    encoder
        .set_pledged_src_size(Some(bytes.len() as u64))
        .expect("Zstandard size");
    encoder.write_all(bytes).expect("Zstandard in memory");
    encoder.finish().expect("Zstandard in memory")
}

fn unzstd(bytes: &[u8]) -> Result<Vec<u8>, String> {
    zstd::stream::decode_all(bytes).map_err(|e| e.to_string())
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
    encoder.write_all(bytes).expect("gzip in memory");
    encoder.finish().expect("gzip in memory")
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh() -> Mesh {
        let stl = {
            let mut s = vec![0u8; 80];
            s.extend_from_slice(&2u32.to_le_bytes());
            let tris: [[[f32; 3]; 3]; 2] = [
                [[0.0, 0.0, 0.0], [1.5, -0.0, 0.0], [0.0, 1e-7, 3.25]],
                [
                    [0.1, 0.2, 0.3],
                    [0.0, 0.0, 0.0],
                    [f32::MAX, f32::MIN_POSITIVE, -2.5],
                ],
            ];
            for t in tris {
                s.extend_from_slice(&[0; 12]);
                for p in t {
                    for x in p {
                        s.extend_from_slice(&x.to_le_bytes());
                    }
                }
                s.extend_from_slice(&[0, 0]);
            }
            s
        };
        read_stl(&stl).expect("an STL")
    }

    #[test]
    fn every_format_gives_back_every_corner() {
        let mesh = mesh();
        let stl = write_stl(&mesh);
        let want = mesh_corners(&mesh).expect("corners");
        let options = Options { zstd_threads: 2 };
        for format in FORMATS {
            let bytes = format.write(&mesh, &stl, &options);
            assert_eq!(
                format.read(&bytes).expect(format.key),
                want,
                "{}",
                format.key
            );
        }
    }

    #[test]
    fn attributes_are_found_by_their_whole_name() {
        assert_eq!(attribute("v1=\"3\" v2=\"4\" v3=\"5\"", "v2"), Ok("4"));
        assert_eq!(attribute("v1=\"3\" v2=\"4\" v3=\"5\"", "v1"), Ok("3"));
        assert_eq!(attribute("x=\"1\" y=\"2\" z=\"3\"", "x"), Ok("1"));
        assert!(attribute("x=\"1\"", "y").is_err());
    }
}
