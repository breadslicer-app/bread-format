# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The mesh chunk, MESH, versions 1 and 2 (SPEC 6)."""

from __future__ import annotations

import struct
from array import array
from dataclasses import dataclass

from ._bytes import Bits, Fields, Meta
from .errors import Malformed, NoSuchVertex
from .meshopt import decode_attributes, decode_triangles

_MASK32 = 0xFFFFFFFF


@dataclass(frozen=True)
class Mesh:
    """A triangle mesh, kept exactly (SPEC 6.1).

    ``vertices`` holds x, y and z of each vertex as the bits of their ``f32``,
    so that every coordinate comes back bit for bit, NaN payloads included;
    ``vertex_bytes`` gives them as little-endian ``f32``s, and ``positions``
    as Python floats. ``triangles`` holds the three vertex indices of each
    triangle, first corner first.
    """

    meta: Meta | None
    vertices: array  # 'I': 3 per vertex, the bits of each f32
    triangles: array  # 'I': 3 per triangle

    @property
    def vertex_count(self) -> int:
        return len(self.vertices) // 3

    @property
    def triangle_count(self) -> int:
        return len(self.triangles) // 3

    def vertex_bytes(self) -> bytes:
        """The vertices as little-endian ``f32``s, ready for an array of
        ``float32``."""
        return struct.pack(f"<{len(self.vertices)}I", *self.vertices)

    def triangle_bytes(self) -> bytes:
        """The triangles as little-endian ``u32``s."""
        return struct.pack(f"<{len(self.triangles)}I", *self.triangles)

    def positions(self) -> list[tuple[float, float, float]]:
        """Each vertex as three floats. A signalling NaN may come out quiet:
        use ``vertices`` or ``vertex_bytes`` to keep every bit."""
        floats = struct.unpack(f"<{len(self.vertices)}f", self.vertex_bytes())
        return [floats[i : i + 3] for i in range(0, len(floats), 3)]


def read_mesh(version: int, payload: bytes | memoryview) -> Mesh:
    """Reads and checks a mesh payload of version 1 or 2."""
    if version == 1:
        return _read_v1(payload)
    if version == 2:
        return _read_v2(payload)
    raise ValueError(f"no mesh version {version}")


# --- Version 1 ----------------------------------------------------------


def _read_v1(payload) -> Mesh:
    f = Fields(payload)
    meta = f.meta()
    vertex_count = f.u32()
    vertex_buffer = f.byte_string()
    _check_vertex_bound(vertex_count, vertex_buffer)
    triangle_count = f.u32()
    index_buffer = f.byte_string()
    rotations = [(f.u32(), f.u8()) for _ in range(f.count(5))]
    f.end()

    vertices = _vertex_bits(decode_attributes(vertex_buffer, vertex_count, 12))
    triangles = decode_triangles(index_buffer, triangle_count)

    # SPEC 6.5: each gap after the first is at least 1, and every rotation
    # names a triangle that exists.
    triangle = -1
    for i, (gap, corner) in enumerate(rotations):
        if i and gap == 0:
            raise Malformed
        triangle = gap if i == 0 else triangle + gap
        if triangle >= triangle_count or corner not in (1, 2):
            raise Malformed
        _turn(triangles, triangle, corner)

    _check_vertices(triangles, vertex_count)
    return Mesh(meta, vertices, triangles)


def _check_vertex_bound(vertex_count: int, buffer) -> None:
    # Every block of up to 256 vertices starts with a 3-byte control header
    # (SPEC 6.3), so L bytes hold fewer than 256 L / 3 vertices.
    if 3 * vertex_count >= 256 * len(buffer):
        raise Malformed


def _vertex_bits(raw: bytes) -> array:
    bits = array("I")
    bits.frombytes(raw)
    if bits.itemsize != 4:
        raise RuntimeError("array('I') is not 32 bits here")
    if struct.pack("=I", 1) != struct.pack("<I", 1):
        bits.byteswap()
    return bits


def _turn(triangles: array, triangle: int, corner: int) -> None:
    """(s0, s1, s2) with corner c becomes (sc, sc+1, sc+2), modulo 3."""
    at = 3 * triangle
    s = triangles[at : at + 3]
    triangles[at : at + 3] = array("I", (s[corner % 3], s[(corner + 1) % 3], s[(corner + 2) % 3]))


def _check_vertices(triangles: array, vertex_count: int) -> None:
    if triangles and max(triangles) >= vertex_count:
        raise NoSuchVertex


# --- Version 2 ----------------------------------------------------------


def _read_v2(payload) -> Mesh:
    f = Fields(payload)
    meta = f.meta()
    vertex_count = f.u32()
    triangle_count = f.u32()
    connectivity = f.u8()
    coding = f.u8()
    if connectivity not in (0, 1) or coding not in (0, 1, 2):
        raise Malformed
    index_buffer = f.byte_string()
    permutation = f.byte_string() if connectivity == 1 else None
    corners = f.byte_string()
    vertex_parts = [f.byte_string() for _ in range(3 if coding == 1 else 1)]
    f.end()

    # SPEC 6.7: the triangles in decoding order, turned by their corners.
    triangles = decode_triangles(index_buffer, triangle_count)
    _check_vertices(triangles, vertex_count)
    bits = Bits(corners)
    for t in range(triangle_count):
        corner = bits.read(2)
        if corner == 3:
            raise Malformed
        if corner:
            _turn(triangles, t, corner)
    bits.end()

    # SPEC 6.9: the vertices, in decoding numbering.
    if coding == 0:
        _check_vertex_bound(vertex_count, vertex_parts[0])
        vertices = _vertex_bits(decode_attributes(vertex_parts[0], vertex_count, 12))
    else:
        if coding == 1:
            if vertex_count > 3 * triangle_count:
                raise Malformed
            residuals = _rans(vertex_count, *vertex_parts)
        else:
            residuals = _planes(vertex_count, vertex_parts[0])
        vertices = _unpredict(vertex_count, triangles, residuals)

    if permutation is not None:
        triangles, vertices = _restore_order(triangles, vertices, permutation, vertex_count)
    return Mesh(meta, vertices, triangles)


def _restore_order(decoded: array, vertices: array, permutation, vertex_count: int):
    """Connectivity B (SPEC 6.7): each decoded triangle back to its index,
    then the vertices numbered by first use."""
    t = len(decoded) // 3
    width = (max(t, 2) - 1).bit_length()
    bits = Bits(permutation)
    triangles = array("I", bytes(4 * len(decoded)))
    seen = bytearray(t)
    for j in range(t):
        p = bits.read(width)
        if p >= t or seen[p]:
            raise Malformed
        seen[p] = 1
        triangles[3 * p : 3 * p + 3] = decoded[3 * j : 3 * j + 3]
    bits.end()

    number = {}
    for i, v in enumerate(triangles):
        triangles[i] = number.setdefault(v, len(number))
    if len(number) != vertex_count:
        raise Malformed
    renumbered = array("I", bytes(4 * len(vertices)))
    for old, new in number.items():
        renumbered[3 * new : 3 * new + 3] = vertices[3 * old : 3 * old + 3]
    return triangles, renumbered


def _planes(vertex_count: int, part) -> list[int]:
    """Coding 2 (SPEC 6.9): 12 planes of one byte per vertex. Plane 4c + i
    holds byte i of the residual of component c."""
    if len(part) != 12 * vertex_count:
        raise Malformed
    n = vertex_count
    residuals = []
    for v in range(n):
        for c in range(3):
            residuals.append(sum(part[(4 * c + i) * n + v] << (8 * i) for i in range(4)))
    return residuals


def _rans(vertex_count: int, tables_part, stream_part, raw_part) -> list[int]:
    """Coding 1 (SPEC 6.9): the residuals, x, y then z of each vertex."""
    tables = _rans_tables(tables_part)
    stream = bytes(stream_part)
    raw = Bits(raw_part)
    if len(stream) < 4:
        raise Malformed
    x = int.from_bytes(stream[:4], "big")
    pos = 4
    residuals = []
    previous = 0  # the largest bit length among the residuals of the vertex before
    for _ in range(vertex_count):
        lengths = []
        for c in range(3):
            if c == 0:
                context = previous
            elif c == 1:
                context = lengths[0]
            else:
                context = (lengths[0] + lengths[1] + 1) // 2
            table = tables[34 * c + context]
            if table is None:
                raise Malformed
            lookup, frequencies, starts = table
            slot = x & 4095
            s = lookup[slot]
            x = frequencies[s] * (x >> 12) + slot - starts[s]
            while x < 1 << 23:
                if pos >= len(stream):
                    raise Malformed
                x = (x << 8) | stream[pos]
                pos += 1
            r = _residual(s, raw)
            residuals.append(r)
            lengths.append(r.bit_length())
        previous = max(lengths)
    if pos != len(stream) or x != 1 << 23:
        raise Malformed
    raw.end()
    return residuals


def _residual(s: int, raw: Bits) -> int:
    big = s >> 2  # L, the residual's bit length
    if big <= 1:
        if s & 3:
            raise Malformed
        return big
    n = big - 1
    h = min(n, 2)
    m = n - h
    if (s & 3) & ((1 << (2 - h)) - 1):
        raise Malformed
    return (1 << n) + (((s & 3) >> (2 - h)) << m) + raw.read(m)


def _rans_tables(part) -> list:
    bits = Bits(part)
    tables: list = []
    for _ in range(102):
        if not bits.read(1):
            tables.append(None)
            continue
        frequencies = [bits.read(12) + 1 if bits.read(1) else 0 for _ in range(132)]
        if sum(frequencies) != 4096:
            raise Malformed
        starts = []
        lookup = bytearray(4096)
        total = 0
        for s, frequency in enumerate(frequencies):
            starts.append(total)
            lookup[total : total + frequency] = bytes([s]) * frequency
            total += frequency
        tables.append((lookup, frequencies, starts))
    bits.end()
    return tables


# --- Vertex prediction (SPEC 6.8) ----------------------------------------


def _unpredict(vertex_count: int, triangles: array, residuals: list[int]) -> array:
    t = len(triangles) // 3
    tris = [tuple(triangles[3 * j : 3 * j + 3]) for j in range(t)]
    # The triangles that have each vertex as a corner, and the triangles on
    # each edge, in increasing order.
    around: list[list[int]] = [[] for _ in range(vertex_count)]
    edges: dict[tuple[int, int], list[int]] = {}
    for j, tri in enumerate(tris):
        for v in dict.fromkeys(tri):
            around[v].append(j)
        for k in range(3):
            a, b = tri[k], tri[(k + 1) % 3]
            on_edge = edges.setdefault((min(a, b), max(a, b)), [])
            if not on_edge or on_edge[-1] != j:
                on_edge.append(j)

    bits = array("I", bytes(12 * vertex_count))
    coords: list[tuple[float, float, float]] = []
    for v in range(vertex_count):
        prediction = _predict(v, tris, around[v], edges, coords, bits)
        for c in range(3):
            r = residuals[3 * v + c]
            d = (r >> 1) ^ (_MASK32 if r & 1 else 0)  # zigzag undone, as a u32
            bits[3 * v + c] = _from_ord((_ord(prediction[c]) + d) & _MASK32)
        coords.append(struct.unpack("<3f", struct.pack("<3I", *bits[3 * v : 3 * v + 3])))
    return bits


def _predict(v, tris, around, edges, coords, bits) -> tuple[int, int, int]:
    """The prediction of vertex v, as the bits of three f32."""
    sums = {"para": [0.0, 0.0, 0.0], "mid": [0.0, 0.0, 0.0], "near": [0.0, 0.0, 0.0]}
    counts = {"para": 0, "mid": 0, "near": 0}

    def add(name, value):
        s = sums[name]
        s[0], s[1], s[2] = s[0] + value[0], s[1] + value[1], s[2] + value[2]
        counts[name] += 1

    for j in around:
        tri = tris[j]
        k = tri.index(v)
        a, b = tri[(k + 1) % 3], tri[(k + 2) % 3]
        if a < v:
            add("near", coords[a])
        if b < v:
            add("near", coords[b])
        if a < v and b < v:
            A, B = coords[a], coords[b]
            add("mid", (0.5 * (A[0] + B[0]), 0.5 * (A[1] + B[1]), 0.5 * (A[2] + B[2])))
            other = next((o for o in edges[(min(a, b), max(a, b))] if o != j), None)
            if other is not None:
                c = next((x for x in tris[other] if x != a and x != b), None)
                if c is not None and c < v:
                    C = coords[c]
                    add("para", ((A[0] + B[0]) - C[0], (A[1] + B[1]) - C[1], (A[2] + B[2]) - C[2]))

    for name in ("para", "mid", "near"):
        if counts[name]:
            return tuple(_to_f32_bits(s / counts[name]) for s in sums[name])
    if v == 0:
        return (0, 0, 0)
    return tuple(b if b & 0x7F800000 != 0x7F800000 else 0 for b in bits[3 * v - 3 : 3 * v])


def _to_f32_bits(value: float) -> int:
    """Rounded to f32, to nearest, ties to even; not finite becomes +0.0."""
    try:
        (out,) = struct.unpack("<I", struct.pack("<f", value))
    except OverflowError:  # past the largest f32: rounds to infinity
        return 0
    return 0 if out & 0x7F800000 == 0x7F800000 else out


def _ord(u: int) -> int:
    return (~u & _MASK32) if u >> 31 else u | 0x80000000


def _from_ord(o: int) -> int:
    return o & 0x7FFFFFFF if o >> 31 else ~o & _MASK32
