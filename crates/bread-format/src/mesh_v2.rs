/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Mesh chunk version 2 (`SPEC.md`, sections 6.6 to 6.9): the same mesh as
//! version 1, kept as exactly, in fewer bytes.
//!
//! Triangles go in their own order (connectivity A) or in the order the
//! triangle codec compresses best, with the way back (connectivity B).
//! Vertices go through meshoptimizer's vertex codec, or are predicted from
//! the triangles around them and only the difference is stored: entropy-coded
//! with rANS, or as byte planes for the chunk's zstd.
//!
//! ```text
//! metadata      as in version 1
//! counts        u32 vertices, u32 triangles
//! codings       u8 connectivity (0 A, 1 B), u8 vertex coding
//!               (0 meshopt, 1 predicted + rANS, 2 predicted + planes)
//! parts         u32 length + bytes each:
//!               A: index buffer, corners
//!               B: index buffer, permutation, corners
//!               then meshopt: vertex buffer; rANS: tables, stream, raw
//!               bits; planes: 12 byte planes
//! ```

use crate::bytes::{In, Out, fits_u32};
use crate::mesh::{DecodeMeshError, EncodeMeshError, Mesh};
use crate::meta::{read_meta, write_meta};

/// The first byte of a vertex buffer: attribute codec version 1.
const VERTEX_HEADER: u8 = 0xa1;
/// The first byte of an index buffer: triangle codec version 1.
const INDEX_HEADER: u8 = 0xe1;
const VERTEX_VERSION: i32 = 1;
const VERTEX_LEVEL: i32 = 2;
const VERTICES_PER_BLOCK: usize = 256;
const BLOCK_HEADER_BYTES: usize = 3;
const INDEX_BUFFER_OVERHEAD: usize = 17;

/// How a version 2 mesh stores its triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connectivity {
    /// In their own order, through the triangle codec, with each first
    /// corner in 2 bits.
    InOrder,
    /// Reordered for the triangle codec, with the permutation back to their
    /// order and each first corner.
    Reordered,
}

/// How a version 2 mesh stores its vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexCoding {
    /// meshoptimizer's vertex codec, version 1.
    Meshopt,
    /// Predicted from the triangles, the differences coded with rANS.
    Rans,
    /// Predicted from the triangles, the differences as 12 byte planes.
    Planes,
}

/// The codings a version 2 mesh payload uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshCoding {
    pub connectivity: Connectivity,
    pub vertices: VertexCoding,
}

// ---------------------------------------------------------------- bits

/// Writes values least significant bit first.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acc: u64,
    count: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, bits: u32) {
        if bits == 0 {
            return;
        }
        self.acc |= (u64::from(value) & ((1u64 << bits) - 1)) << self.count;
        self.count += bits;
        while self.count >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.count -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.bytes.push(self.acc as u8);
        }
        self.bytes
    }
}

/// Reads what a [`BitWriter`] wrote, and nothing else: [`BitReader::done`]
/// checks that every byte was needed and the padding is zero.
struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn get(&mut self, bits: u32) -> Option<u32> {
        if bits == 0 {
            return Some(0);
        }
        let byte = self.pos >> 3;
        let value = match self.bytes.get(byte..byte + 8) {
            // Eight whole bytes ahead: read them at once.
            Some(word) => {
                let word = u64::from_le_bytes(word.try_into().ok()?);
                (word >> (self.pos & 7)) as u32 & ((1u64 << bits) - 1) as u32
            }
            // Near the end: bit by bit, so running out is noticed.
            None => {
                let mut value = 0u32;
                for i in 0..bits {
                    let byte = *self.bytes.get((self.pos + i as usize) >> 3)?;
                    value |= u32::from((byte >> ((self.pos + i as usize) & 7)) & 1) << i;
                }
                value
            }
        };
        self.pos += bits as usize;
        Some(value)
    }

    fn done(&self) -> bool {
        let used = self.pos.div_ceil(8);
        used == self.bytes.len()
            && (self.pos & 7 == 0 || self.bytes[used - 1] >> (self.pos & 7) == 0)
    }
}

/// The number of bits each permutation entry takes: enough for `n - 1`, and
/// at least 1.
fn permutation_bits(triangles: usize) -> u32 {
    usize::BITS - (triangles.max(2) - 1).leading_zeros()
}

// ---------------------------------------------------------------- prediction

/// `f32` bits mapped to a `u32` that orders as the values do.
fn ord(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits | 0x8000_0000
    }
}

fn unord(ordered: u32) -> f32 {
    f32::from_bits(if ordered & 0x8000_0000 != 0 {
        ordered & 0x7fff_ffff
    } else {
        !ordered
    })
}

fn zigzag(value: u32) -> u32 {
    let value = value as i32;
    ((value << 1) ^ (value >> 31)) as u32
}

fn unzigzag(value: u32) -> u32 {
    (value >> 1) ^ (value & 1).wrapping_neg()
}

/// The triangles around each vertex, and across each edge, of a triangle
/// list. Built in linear time whatever the triangles, so a damaged or hostile
/// mesh cannot make prediction slow.
struct Topology {
    /// For vertex `v`, `around[start[v]..start[v + 1]]` lists the triangles
    /// that have it as a corner, each once, in increasing order.
    start: Vec<u32>,
    around: Vec<u32>,
    /// For corner `k` of triangle `t`, at `3 t + k`: the first triangle other
    /// than `t`, in increasing order, with an edge between the other two
    /// corners, or `u32::MAX`.
    across: Vec<u32>,
}

const NONE: u32 = u32::MAX;

fn topology(triangles: &[[u32; 3]], vertex_count: usize) -> Topology {
    // Triangles around each vertex, a triangle with a repeated corner once.
    let mut start = vec![0u32; vertex_count + 1];
    for t in triangles {
        for (k, &v) in t.iter().enumerate() {
            if !t[..k].contains(&v) {
                start[v as usize + 1] += 1;
            }
        }
    }
    for v in 0..vertex_count {
        start[v + 1] += start[v];
    }
    let mut fill = start.clone();
    let mut around = vec![0u32; start[vertex_count] as usize];
    for (i, t) in triangles.iter().enumerate() {
        for (k, &v) in t.iter().enumerate() {
            if !t[..k].contains(&v) {
                around[fill[v as usize] as usize] = i as u32;
                fill[v as usize] += 1;
            }
        }
    }

    // Edges grouped by their lower vertex, then sorted by their higher one:
    // the per-vertex lists are short in any mesh, and the total work is
    // bounded by the triangle count whatever their degrees.
    let mut edge_start = vec![0u32; vertex_count + 1];
    for t in triangles {
        for k in 0..3 {
            let low = t[(k + 1) % 3].min(t[(k + 2) % 3]);
            edge_start[low as usize + 1] += 1;
        }
    }
    for v in 0..vertex_count {
        edge_start[v + 1] += edge_start[v];
    }
    let mut fill = edge_start.clone();
    // The higher vertex in the top half, the triangle's corner `3 t + k` in
    // the bottom one, so sorting orders by edge, then by triangle.
    let mut edges = vec![0u64; 3 * triangles.len()];
    for (i, t) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[(k + 1) % 3], t[(k + 2) % 3]);
            let slot = &mut fill[a.min(b) as usize];
            edges[*slot as usize] = (u64::from(a.max(b)) << 32) | (3 * i + k) as u64;
            *slot += 1;
        }
    }
    let mut across = vec![NONE; 3 * triangles.len()];
    for v in 0..vertex_count {
        let list = &mut edges[edge_start[v] as usize..edge_start[v + 1] as usize];
        list.sort_unstable();
        for group in list.chunk_by(|x, y| x >> 32 == y >> 32) {
            let triangle = |e: &u64| (*e as u32) / 3;
            // The first triangle on the edge, and the first one after it:
            // a triangle meets an edge at most three times.
            let first = triangle(&group[0]);
            let second = group
                .iter()
                .map(triangle)
                .find(|&t| t != first)
                .unwrap_or(NONE);
            for e in group {
                let t = triangle(e);
                across[*e as u32 as usize] = if t == first { second } else { first };
            }
        }
    }
    Topology {
        start,
        around,
        across,
    }
}

/// The prediction of vertex `v` from the vertices before it (`SPEC.md`,
/// section 6.8). Every step is in `f64`, and the result is rounded to `f32`
/// once; a result that is not finite becomes `+0.0`.
fn predict(v: u32, triangles: &[[u32; 3]], topo: &Topology, vertices: &[[f32; 3]]) -> [f32; 3] {
    let at = |i: u32| vertices[i as usize].map(f64::from);
    let (mut para, mut para_n) = ([0f64; 3], 0u32);
    let (mut mid, mut mid_n) = ([0f64; 3], 0u32);
    let (mut near, mut near_n) = ([0f64; 3], 0u32);
    let list = &topo.around[topo.start[v as usize] as usize..topo.start[v as usize + 1] as usize];
    for &t in list {
        let tri = triangles[t as usize];
        let k = tri.iter().position(|&c| c == v).unwrap_or(0);
        let (a, b) = (tri[(k + 1) % 3], tri[(k + 2) % 3]);
        let (known_a, known_b) = (a < v, b < v);
        if known_a {
            let p = at(a);
            (0..3).for_each(|i| near[i] += p[i]);
            near_n += 1;
        }
        if known_b {
            let p = at(b);
            (0..3).for_each(|i| near[i] += p[i]);
            near_n += 1;
        }
        if known_a && known_b {
            let (pa, pb) = (at(a), at(b));
            (0..3).for_each(|i| mid[i] += 0.5 * (pa[i] + pb[i]));
            mid_n += 1;
            let other = topo.across[3 * t as usize + k];
            if other != NONE {
                let opposite = triangles[other as usize]
                    .into_iter()
                    .find(|&c| c != a && c != b);
                if let Some(c) = opposite.filter(|&c| c < v) {
                    let pc = at(c);
                    (0..3).for_each(|i| para[i] += (pa[i] + pb[i]) - pc[i]);
                    para_n += 1;
                }
            }
        }
    }
    let (sum, n) = if para_n > 0 {
        (para, para_n)
    } else if mid_n > 0 {
        (mid, mid_n)
    } else if near_n > 0 {
        (near, near_n)
    } else if v > 0 {
        (at(v - 1), 1)
    } else {
        ([0.0; 3], 1)
    };
    sum.map(|s| {
        let p = (s / f64::from(n)) as f32;
        if p.is_finite() { p } else { 0.0 }
    })
}

/// The residuals of `vertices`, in vertex order, each component zigzagged.
fn residuals(vertices: &[[f32; 3]], triangles: &[[u32; 3]]) -> Vec<[u32; 3]> {
    let topo = topology(triangles, vertices.len());
    (0..vertices.len())
        .map(|v| {
            let p = predict(v as u32, triangles, &topo, vertices);
            std::array::from_fn(|i| zigzag(ord(vertices[v][i]).wrapping_sub(ord(p[i]))))
        })
        .collect()
}

fn rebuild(residuals: &[[u32; 3]], triangles: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let topo = topology(triangles, residuals.len());
    let mut vertices = vec![[0f32; 3]; residuals.len()];
    for v in 0..residuals.len() {
        let p = predict(v as u32, triangles, &topo, &vertices);
        vertices[v] =
            std::array::from_fn(|i| unord(ord(p[i]).wrapping_add(unzigzag(residuals[v][i]))));
    }
    vertices
}

// ---------------------------------------------------------------- rANS

const PROB_BITS: u32 = 12;
const PROB_TOTAL: u32 = 1 << PROB_BITS;
const RANS_LOW: u32 = 1 << 23;
/// High bits of a residual folded into its symbol, besides its bit length.
const TOP_BITS: u32 = 2;
const SYMBOLS: usize = 33 << TOP_BITS;
const BUCKETS: usize = 34;
const CONTEXTS: usize = 3 * BUCKETS;

fn bit_length(r: u32) -> u32 {
    u32::BITS - r.leading_zeros()
}

/// The context of component `comp` of a residual: the bit lengths of what
/// came before it.
fn context(comp: usize, current: &[u32; 3], previous: &[u32; 3]) -> usize {
    let bucket = match comp {
        0 => previous.iter().map(|&r| bit_length(r)).max().unwrap_or(0),
        1 => bit_length(current[0]),
        _ => (bit_length(current[0]) + bit_length(current[1])).div_ceil(2),
    };
    comp * BUCKETS + bucket as usize
}

/// A residual as a symbol (its bit length and up to two bits under the top
/// one) and the raw bits below those.
fn symbol(r: u32) -> (u32, u32, u32) {
    let length = bit_length(r);
    if length <= 1 {
        return (length << TOP_BITS, 0, 0);
    }
    let low = length - 1;
    let top = low.min(TOP_BITS);
    let raw_bits = low - top;
    let high = ((r >> raw_bits) & ((1 << top) - 1)) << (TOP_BITS - top);
    let raw = r & ((1u64 << raw_bits) - 1) as u32;
    ((length << TOP_BITS) | high, raw, raw_bits)
}

/// The residual a symbol and its raw bits stand for, or `None` for a symbol
/// no writer produces.
fn residual(symbol: u32, raw: &mut BitReader<'_>) -> Option<u32> {
    let length = symbol >> TOP_BITS;
    let high = symbol & ((1 << TOP_BITS) - 1);
    if length <= 1 {
        return (high == 0).then_some(length);
    }
    let low = length - 1;
    let top = low.min(TOP_BITS);
    if high & ((1 << (TOP_BITS - top)) - 1) != 0 {
        return None;
    }
    let raw_bits = low - top;
    Some((1 << low) | ((high >> (TOP_BITS - top)) << raw_bits) | raw.get(raw_bits)?)
}

/// Symbol frequencies summing to `PROB_TOTAL`, each used symbol at least 1.
fn normalize(counts: &[u64]) -> Vec<u32> {
    let total: u64 = counts.iter().sum();
    let mut freq: Vec<u32> = counts
        .iter()
        .map(|&c| {
            if c == 0 {
                0
            } else {
                ((c * u64::from(PROB_TOTAL)) / total).max(1) as u32
            }
        })
        .collect();
    let mut sum: i64 = freq.iter().map(|&f| i64::from(f)).sum();
    while sum != i64::from(PROB_TOTAL) {
        let (i, _) = freq
            .iter()
            .enumerate()
            .max_by_key(|&(i, &f)| (f, std::cmp::Reverse(i)))
            .expect("a used context has a symbol");
        if sum > i64::from(PROB_TOTAL) {
            let take = (sum - i64::from(PROB_TOTAL)).min(i64::from(freq[i]) - 1);
            freq[i] -= take as u32;
            sum -= take;
        } else {
            freq[i] += (i64::from(PROB_TOTAL) - sum) as u32;
            sum = i64::from(PROB_TOTAL);
        }
    }
    freq
}

/// A context's table: frequencies, their running sums, and the symbol of
/// every slot.
struct Table {
    freq: Vec<u32>,
    cum: Vec<u32>,
    slots: Vec<u16>,
}

fn table(freq: Vec<u32>) -> Table {
    let mut cum = vec![0u32; freq.len() + 1];
    for s in 0..freq.len() {
        cum[s + 1] = cum[s] + freq[s];
    }
    let mut slots = vec![0u16; PROB_TOTAL as usize];
    for s in 0..freq.len() {
        slots[cum[s] as usize..cum[s + 1] as usize].fill(s as u16);
    }
    Table { freq, cum, slots }
}

/// Codes residuals as (tables, stream, raw bits).
fn encode_rans(residuals: &[[u32; 3]]) -> [Vec<u8>; 3] {
    let mut counts = vec![0u64; CONTEXTS * SYMBOLS];
    let mut items = Vec::with_capacity(3 * residuals.len());
    let mut raw = BitWriter::default();
    let mut previous = [0u32; 3];
    for r in residuals {
        for comp in 0..3 {
            let c = context(comp, r, &previous);
            let (s, bits, n) = symbol(r[comp]);
            counts[c * SYMBOLS + s as usize] += 1;
            items.push((c, s as usize));
            raw.put(bits, n);
        }
        previous = *r;
    }
    let mut tables_out = BitWriter::default();
    let mut tables = Vec::with_capacity(CONTEXTS);
    for c in 0..CONTEXTS {
        let counts = &counts[c * SYMBOLS..(c + 1) * SYMBOLS];
        if counts.iter().all(|&n| n == 0) {
            tables_out.put(0, 1);
            tables.push(None);
            continue;
        }
        tables_out.put(1, 1);
        let freq = normalize(counts);
        for &f in &freq {
            if f == 0 {
                tables_out.put(0, 1);
            } else {
                tables_out.put(1, 1);
                tables_out.put(f - 1, PROB_BITS);
            }
        }
        tables.push(Some(table(freq)));
    }
    // rANS codes backwards, so the decoder reads forwards.
    let mut x = RANS_LOW;
    let mut stream = Vec::with_capacity(items.len());
    for &(c, s) in items.iter().rev() {
        let t = tables[c].as_ref().expect("a used context has a table");
        let (f, start) = (t.freq[s], t.cum[s]);
        let limit = ((RANS_LOW >> PROB_BITS) << 8) * f;
        while x >= limit {
            stream.push(x as u8);
            x >>= 8;
        }
        x = ((x / f) << PROB_BITS) + (x % f) + start;
    }
    stream.extend(x.to_le_bytes());
    stream.reverse();
    [tables_out.finish(), stream, raw.finish()]
}

fn decode_rans(
    tables: &[u8],
    stream: &[u8],
    raw: &[u8],
    vertex_count: usize,
) -> Option<Vec<[u32; 3]>> {
    let mut bits = BitReader::new(tables);
    let mut parsed: Vec<Option<Table>> = Vec::with_capacity(CONTEXTS);
    for _ in 0..CONTEXTS {
        if bits.get(1)? == 0 {
            parsed.push(None);
            continue;
        }
        let mut freq = Vec::with_capacity(SYMBOLS);
        for _ in 0..SYMBOLS {
            freq.push(if bits.get(1)? == 0 {
                0
            } else {
                bits.get(PROB_BITS)? + 1
            });
        }
        if freq.iter().sum::<u32>() != PROB_TOTAL {
            return None;
        }
        parsed.push(Some(table(freq)));
    }
    if !bits.done() {
        return None;
    }
    let (head, mut rest) = stream.split_first_chunk::<4>()?;
    let mut x = u64::from(u32::from_be_bytes(*head));
    let mut raw = BitReader::new(raw);
    let mut out = Vec::with_capacity(vertex_count);
    let mut previous = [0u32; 3];
    for _ in 0..vertex_count {
        let mut r = [0u32; 3];
        for comp in 0..3 {
            let t = parsed[context(comp, &r, &previous)].as_ref()?;
            let slot = (x & u64::from(PROB_TOTAL - 1)) as u32;
            let s = usize::from(t.slots[slot as usize]);
            x = u64::from(t.freq[s]) * (x >> PROB_BITS) + u64::from(slot - t.cum[s]);
            while x < u64::from(RANS_LOW) {
                let (&byte, next) = rest.split_first()?;
                x = (x << 8) | u64::from(byte);
                rest = next;
            }
            r[comp] = residual(s as u32, &mut raw)?;
        }
        out.push(r);
        previous = r;
    }
    (x == u64::from(RANS_LOW) && rest.is_empty() && raw.done()).then_some(out)
}

// ---------------------------------------------------------------- planes

fn encode_planes(residuals: &[[u32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 * residuals.len());
    for comp in 0..3 {
        for byte in 0..4 {
            out.extend(residuals.iter().map(|r| (r[comp] >> (8 * byte)) as u8));
        }
    }
    out
}

fn decode_planes(planes: &[u8], vertex_count: usize) -> Option<Vec<[u32; 3]>> {
    if Some(planes.len()) != vertex_count.checked_mul(12) {
        return None;
    }
    let mut out = vec![[0u32; 3]; vertex_count];
    for comp in 0..3 {
        for byte in 0..4 {
            let plane = &planes[(4 * comp + byte) * vertex_count..][..vertex_count];
            for (r, &b) in out.iter_mut().zip(plane) {
                r[comp] |= u32::from(b) << (8 * byte);
            }
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- triangles

/// The corner a stored triangle has to start from to be `original`: 0, 1 or
/// 2, with `original[k] == stored[(k + r) % 3]`.
fn corner(stored: &[u32], original: &[u32; 3]) -> u32 {
    (0..3u32)
        .find(|&r| (0..3).all(|k| stored[(k + r as usize) % 3] == original[k]))
        .expect("the codec only rotates triangles")
}

fn encode_corners(stored: &[u32], triangles: &[[u32; 3]]) -> Vec<u8> {
    let mut out = BitWriter::default();
    for (s, t) in stored.as_chunks::<3>().0.iter().zip(triangles) {
        out.put(corner(s, t), 2);
    }
    out.finish()
}

/// The stored triangles, each turned to start from its corner.
fn apply_corners(stored: &[u32], corners: &[u8]) -> Option<Vec<[u32; 3]>> {
    let mut bits = BitReader::new(corners);
    let mut out = Vec::with_capacity(stored.len() / 3);
    for s in stored.as_chunks::<3>().0 {
        let r = bits.get(2)? as usize;
        if r > 2 {
            return None;
        }
        out.push(std::array::from_fn(|k| s[(k + r) % 3]));
    }
    bits.done().then_some(out)
}

fn encode_index_buffer(triangles: &[[u32; 3]], vertex_count: usize) -> (Vec<u8>, Vec<u32>) {
    let encoded = meshopt::encode_index_buffer(triangles.as_flattened(), vertex_count)
        .expect("whole triangles encode");
    assert_eq!(
        encoded.first(),
        Some(&INDEX_HEADER),
        "the triangle codec writes version 1"
    );
    let stored = meshopt::decode_index_buffer(&encoded, 3 * triangles.len())
        .expect("the codec reads its own output");
    (encoded, stored)
}

/// Whether connectivity B can store `mesh`: its vertices are numbered in the
/// order its triangles first use them, and every vertex is used.
pub(crate) fn in_first_use_order(mesh: &Mesh) -> bool {
    let mut next = 0u32;
    for &v in mesh.triangles.as_flattened() {
        if v == next {
            next += 1;
        } else if v > next {
            return false;
        }
    }
    next as usize == mesh.vertices.len()
}

/// Connectivity B's order: the triangles reordered for the codec, with the
/// vertices renumbered by first use in that order. Returns the reordered
/// triangles (their own first corners) and vertices, and the original index
/// of each reordered triangle.
fn reorder(mesh: &Mesh) -> (Vec<[u32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let n = mesh.vertices.len();
    let order = meshopt::optimize_vertex_cache(mesh.triangles.as_flattened(), n);
    // Match each reordered triangle to an original one by a key that does
    // not depend on the first corner (the smallest of its three rotations);
    // equal triangles match in any order.
    let key = |t: &[u32]| -> (u32, u32, u32) {
        (0..3)
            .map(|r| (t[r], t[(r + 1) % 3], t[(r + 2) % 3]))
            .min()
            .unwrap_or_default()
    };
    let mut originals: Vec<_> = mesh
        .triangles
        .iter()
        .enumerate()
        .map(|(i, t)| (key(t), i as u32))
        .collect();
    let mut reordered: Vec<_> = order
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(j, t)| (key(t), j as u32))
        .collect();
    originals.sort_unstable();
    reordered.sort_unstable();
    let mut permutation = vec![0u32; mesh.triangles.len()];
    for (o, r) in originals.iter().zip(&reordered) {
        debug_assert_eq!(o.0, r.0);
        permutation[r.1 as usize] = o.1;
    }
    let mut renumber = vec![NONE; n];
    let mut vertices = Vec::with_capacity(n);
    let triangles = permutation
        .iter()
        .map(|&p| {
            mesh.triangles[p as usize].map(|v| {
                let new = &mut renumber[v as usize];
                if *new == NONE {
                    *new = vertices.len() as u32;
                    vertices.push(mesh.vertices[v as usize]);
                }
                *new
            })
        })
        .collect();
    (triangles, vertices, permutation)
}

// ---------------------------------------------------------------- payload

/// The triangles and vertices of `mesh` as the vertex coding sees them, and
/// the connectivity parts.
struct Prepared {
    parts: Vec<Vec<u8>>,
    triangles: Vec<[u32; 3]>,
    vertices: Vec<[f32; 3]>,
}

fn prepare(mesh: &Mesh, connectivity: Connectivity) -> Prepared {
    let n = mesh.vertices.len();
    match connectivity {
        Connectivity::InOrder => {
            let (index, stored) = encode_index_buffer(&mesh.triangles, n);
            let corners = encode_corners(&stored, &mesh.triangles);
            Prepared {
                parts: vec![index, corners],
                triangles: mesh.triangles.clone(),
                vertices: mesh.vertices.clone(),
            }
        }
        Connectivity::Reordered => {
            let (triangles, vertices, permutation) = reorder(mesh);
            let (index, stored) = encode_index_buffer(&triangles, n);
            let bits = permutation_bits(triangles.len());
            let mut perm = BitWriter::default();
            for &p in &permutation {
                perm.put(p, bits);
            }
            let corners = encode_corners(&stored, &triangles);
            Prepared {
                parts: vec![index, perm.finish(), corners],
                triangles,
                vertices,
            }
        }
    }
}

fn vertex_parts(prepared: &Prepared, coding: VertexCoding, residuals: &[[u32; 3]]) -> Vec<Vec<u8>> {
    match coding {
        VertexCoding::Meshopt => vec![
            meshopt::encode_vertex_buffer_level(
                &prepared.vertices,
                VERTEX_LEVEL,
                Some(VERTEX_VERSION),
            )
            .expect("12-byte vertices encode"),
        ],
        VertexCoding::Rans => encode_rans(residuals).into(),
        VertexCoding::Planes => vec![encode_planes(residuals)],
    }
}

fn assemble(
    mesh: &Mesh,
    coding: MeshCoding,
    parts: &[Vec<u8>],
) -> Result<Vec<u8>, EncodeMeshError> {
    if parts.iter().any(|part| !fits_u32(part.len())) {
        return Err(EncodeMeshError::TooLarge);
    }
    let mut out = Out::default();
    write_meta(&mut out, mesh.meta.as_ref());
    out.len(mesh.vertices.len());
    out.len(mesh.triangles.len());
    out.u8(match coding.connectivity {
        Connectivity::InOrder => 0,
        Connectivity::Reordered => 1,
    });
    out.u8(match coding.vertices {
        VertexCoding::Meshopt => 0,
        VertexCoding::Rans => 1,
        VertexCoding::Planes => 2,
    });
    for part in parts {
        out.bytes(part);
    }
    Ok(out.0)
}

/// What every coding needs of a mesh, and whether `coding` can store it.
fn check(mesh: &Mesh, coding: MeshCoding) -> Result<(), EncodeMeshError> {
    let n = mesh.vertices.len();
    if !fits_u32(n) || !fits_u32(mesh.triangles.len()) {
        return Err(EncodeMeshError::TooLarge);
    }
    if mesh
        .triangles
        .as_flattened()
        .iter()
        .any(|&v| v as usize >= n)
    {
        return Err(EncodeMeshError::VertexOutOfRange);
    }
    if coding.connectivity == Connectivity::Reordered && !in_first_use_order(mesh) {
        return Err(EncodeMeshError::Unsupported);
    }
    if coding.vertices == VertexCoding::Rans && n > 3 * mesh.triangles.len() {
        return Err(EncodeMeshError::Unsupported);
    }
    Ok(())
}

/// Encodes `mesh` as a mesh chunk payload of version 2 with the given
/// codings.
///
/// Connectivity B needs the vertices in first-use order, every one used; the
/// rANS coding needs no more than three vertices per triangle.
pub fn encode_mesh_v2_with(mesh: &Mesh, coding: MeshCoding) -> Result<Vec<u8>, EncodeMeshError> {
    check(mesh, coding)?;
    let prepared = prepare(mesh, coding.connectivity);
    let residuals = match coding.vertices {
        VertexCoding::Meshopt => Vec::new(),
        _ => residuals(&prepared.vertices, &prepared.triangles),
    };
    let mut parts = prepared.parts.clone();
    parts.extend(vertex_parts(&prepared, coding.vertices, &residuals));
    assemble(mesh, coding, &parts)
}

/// Decodes a mesh chunk payload of version 2.
pub fn decode_mesh_v2(payload: &[u8]) -> Result<Mesh, DecodeMeshError> {
    decode(&mut In(payload)).ok_or(DecodeMeshError::Malformed)?
}

fn decode(input: &mut In<'_>) -> Option<Result<Mesh, DecodeMeshError>> {
    let meta = read_meta(input)?;
    let n = input.len()?;
    let nt = input.len()?;
    let connectivity = match input.u8()? {
        0 => Connectivity::InOrder,
        1 => Connectivity::Reordered,
        _ => return None,
    };
    let coding = match input.u8()? {
        0 => VertexCoding::Meshopt,
        1 => VertexCoding::Rans,
        2 => VertexCoding::Planes,
        _ => return None,
    };
    let index = input.bytes()?;
    if index.first() != Some(&INDEX_HEADER)
        || nt > index.len().checked_sub(INDEX_BUFFER_OVERHEAD)?
    {
        return None;
    }
    let permutation = match connectivity {
        Connectivity::InOrder => None,
        Connectivity::Reordered => Some(input.bytes()?),
    };
    let corners = input.bytes()?;
    // Bound the vertex count by the coding before reserving anything.
    let vertex_parts: Vec<&[u8]> = match coding {
        VertexCoding::Meshopt => {
            let buffer = input.bytes()?;
            if buffer.first() != Some(&VERTEX_HEADER)
                || n.checked_mul(BLOCK_HEADER_BYTES)?
                    >= buffer.len().checked_mul(VERTICES_PER_BLOCK)?
            {
                return None;
            }
            vec![buffer]
        }
        VertexCoding::Rans => {
            if n > nt.checked_mul(3)? {
                return None;
            }
            vec![input.bytes()?, input.bytes()?, input.bytes()?]
        }
        VertexCoding::Planes => {
            let planes = input.bytes()?;
            if Some(planes.len()) != n.checked_mul(12) {
                return None;
            }
            vec![planes]
        }
    };
    if !input.is_empty() {
        return None;
    }

    let stored: Vec<u32> = meshopt::decode_index_buffer(index, nt.checked_mul(3)?).ok()?;
    let triangles = apply_corners(&stored, corners)?;
    if triangles.as_flattened().iter().any(|&v| v as usize >= n) {
        return Some(Err(DecodeMeshError::VertexOutOfRange));
    }
    let vertices = match coding {
        VertexCoding::Meshopt => meshopt::decode_vertex_buffer(vertex_parts[0], n).ok()?,
        VertexCoding::Rans => rebuild(
            &decode_rans(vertex_parts[0], vertex_parts[1], vertex_parts[2], n)?,
            &triangles,
        ),
        VertexCoding::Planes => rebuild(&decode_planes(vertex_parts[0], n)?, &triangles),
    };
    let mesh = match permutation {
        None => Mesh {
            meta: None,
            vertices,
            triangles,
        },
        Some(bytes) => restore_order(bytes, vertices, &triangles)?,
    };
    Some(Ok(Mesh { meta, ..mesh }))
}

/// Connectivity B: puts the triangles back in their order, and numbers the
/// vertices by first use in it.
fn restore_order(bytes: &[u8], vertices: Vec<[f32; 3]>, triangles: &[[u32; 3]]) -> Option<Mesh> {
    let nt = triangles.len();
    let bits = permutation_bits(nt);
    let mut reader = BitReader::new(bytes);
    let mut original = vec![None; nt];
    for t in triangles {
        let p = reader.get(bits)? as usize;
        let slot = original.get_mut(p)?;
        if slot.is_some() {
            return None;
        }
        *slot = Some(*t);
    }
    if !reader.done() {
        return None;
    }
    let mut renumber = vec![NONE; vertices.len()];
    let mut ordered = Vec::with_capacity(vertices.len());
    let triangles = original
        .into_iter()
        .map(|t| {
            t.map(|t| {
                t.map(|v| {
                    let new = &mut renumber[v as usize];
                    if *new == NONE {
                        *new = ordered.len() as u32;
                        ordered.push(vertices[v as usize]);
                    }
                    *new
                })
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (ordered.len() == vertices.len()).then_some(Mesh {
        meta: None,
        vertices: ordered,
        triangles,
    })
}
