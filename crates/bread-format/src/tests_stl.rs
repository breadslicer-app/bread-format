/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::mesh::Mesh;
use crate::meta::Meta;
use crate::stl::{ReadStlError, read_stl};

/// A binary STL of `triangles`, each three corners, with `header` padded
/// with NULs to 80 bytes.
fn binary(header: &[u8], triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut out = header.to_vec();
    out.resize(80, 0);
    out.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for (i, triangle) in triangles.iter().enumerate() {
        // A normal and an attribute that must not matter.
        for value in [0.5f32, -1.0, f32::NAN] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for corner in triangle {
            for value in corner {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out.extend_from_slice(&(i as u16).to_le_bytes());
    }
    out
}

/// The corners a mesh gives back: three per triangle, in order.
fn corners(mesh: &Mesh) -> Vec<[u32; 3]> {
    mesh.triangles
        .as_flattened()
        .iter()
        .map(|&v| mesh.vertices[v as usize].map(f32::to_bits))
        .collect()
}

fn bits(triangles: &[[[f32; 3]; 3]]) -> Vec<[u32; 3]> {
    triangles
        .as_flattened()
        .iter()
        .map(|c| c.map(f32::to_bits))
        .collect()
}

/// Two triangles sharing an edge, and corners that are equal as numbers but
/// not as bits (0.0 and -0.0), which must stay two vertices.
fn square() -> Vec<[[f32; 3]; 3]> {
    vec![
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
        [[-0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.1]],
    ]
}

#[test]
fn a_binary_stl_comes_back_corner_for_corner() {
    let triangles = square();
    let mesh = read_stl(&binary(b"", &triangles)).unwrap();
    assert_eq!(corners(&mesh), bits(&triangles));
    // 0.0 and -0.0 are two vertices; the shared edge is welded.
    assert_eq!(mesh.vertices.len(), 5);
    assert_eq!(mesh.triangles, [[0, 1, 2], [3, 2, 4]]);
    assert_eq!(mesh.meta, None);
}

#[test]
fn every_bit_pattern_is_kept() {
    let odd = [
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
    ];
    let triangles = vec![[
        [odd[0], odd[1], odd[2]],
        [odd[3], odd[4], 0.0],
        [1.0, 2.0, 3.0],
    ]];
    let mesh = read_stl(&binary(b"", &triangles)).unwrap();
    assert_eq!(corners(&mesh), bits(&triangles));
}

#[test]
fn a_binary_header_with_text_is_the_comment() {
    let mesh = read_stl(&binary(b"solid exported part  ", &square())).unwrap();
    assert_eq!(
        mesh.meta,
        Some(Meta {
            name: None,
            comment: Some("solid exported part".to_owned()),
        })
    );
    let mut header = b"text".to_vec();
    header.push(1);
    assert_eq!(read_stl(&binary(&header, &square())).unwrap().meta, None);
    assert_eq!(
        read_stl(&binary(&[0xff; 80], &square())).unwrap().meta,
        None
    );
}

#[test]
fn a_binary_stl_whose_header_says_solid_is_still_binary() {
    let triangles = square();
    let mesh = read_stl(&binary(b"solid", &triangles)).unwrap();
    assert_eq!(corners(&mesh), bits(&triangles));
}

#[test]
fn an_empty_binary_stl_is_an_empty_mesh() {
    let mesh = read_stl(&binary(b"", &[])).unwrap();
    assert!(mesh.vertices.is_empty() && mesh.triangles.is_empty());
}

#[test]
fn a_binary_stl_of_the_wrong_length_is_not_an_stl() {
    let mut bytes = binary(&[0; 80], &square());
    bytes.pop();
    assert_eq!(read_stl(&bytes), Err(ReadStlError::NotStl));
    // A count far past the bytes asks for nothing.
    let mut bytes = binary(&[0; 80], &square());
    bytes[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(read_stl(&bytes), Err(ReadStlError::NotStl));
    assert_eq!(read_stl(b""), Err(ReadStlError::NotStl));
    assert_eq!(read_stl(b"PK\x03\x04 a zip"), Err(ReadStlError::NotStl));
}

const ASCII: &str = "solid part one
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
      vertex 1 1 0
    endloop
  endfacet
  FACET NORMAL 0 0 1
    OUTER LOOP
      VERTEX -0 0 0
      vertex 1e0 1.0 0
      vertex\t0\t1\t0.1
    ENDLOOP
  ENDFACET
endsolid part one
";

#[test]
fn an_ascii_stl_comes_back_corner_for_corner() {
    let mesh = read_stl(ASCII.as_bytes()).unwrap();
    assert_eq!(corners(&mesh), bits(&square()));
    assert_eq!(mesh.triangles, [[0, 1, 2], [3, 2, 4]]);
    assert_eq!(
        mesh.meta,
        Some(Meta {
            name: Some("part one".to_owned()),
            comment: None,
        })
    );
    let crlf = ASCII.replace('\n', "\r\n");
    assert_eq!(read_stl(crlf.as_bytes()).unwrap(), mesh);
}

#[test]
fn an_ascii_stl_without_a_name_has_no_metadata() {
    let text = "solid\nendsolid\n";
    let mesh = read_stl(text.as_bytes()).unwrap();
    assert_eq!(mesh, Mesh::default());
}

#[test]
fn a_broken_ascii_stl_names_its_line() {
    let cases = [
        (ASCII.replace("vertex 1 1 0", "vertex 1 1"), 6),
        (ASCII.replace("vertex 1 1 0", "vertex 1 1 0 0"), 6),
        (ASCII.replace("vertex 1 1 0", "vertex 1 one 0"), 6),
        (
            ASCII.replace("    endloop\n  endfacet\n  FACET", "  endfacet\n  FACET"),
            7,
        ),
        (ASCII.replace("outer loop", "outer"), 3),
        (ASCII.replace("facet normal 0 0 1", "facet normal 0 0"), 2),
        (ASCII.replace("endsolid part one\n", ""), 15),
    ];
    for (text, line) in cases {
        assert_eq!(
            read_stl(text.as_bytes()),
            Err(ReadStlError::Ascii { line }),
            "{text}"
        );
    }
}

#[test]
fn text_that_is_not_an_stl_is_refused() {
    assert_eq!(read_stl(b"solidity\n"), Err(ReadStlError::NotStl));
    assert_eq!(read_stl(b"hello\n"), Err(ReadStlError::NotStl));
    assert_eq!(read_stl(b"\n\n"), Err(ReadStlError::NotStl));
}
