# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The two meshoptimizer codecs a MESH chunk uses (SPEC 6.3, 6.4), written
from their bitstream specification in Appendix A of the glTF extension
KHR_meshopt_compression: "Mode 0: attributes", version 1, and "Mode 1:
triangles"."""

from __future__ import annotations

from array import array

from .errors import Malformed

_MASK32 = 0xFFFFFFFF


class _Cursor:
    """Reads bytes up to ``end``; reading past it is malformed."""

    def __init__(self, data: bytes, pos: int, end: int) -> None:
        self.data = data
        self.pos = pos
        self.end = end

    def take(self, n: int) -> bytes:
        if n > self.end - self.pos:
            raise Malformed
        out = self.data[self.pos : self.pos + n]
        self.pos += n
        return out

    def byte(self) -> int:
        if self.pos >= self.end:
            raise Malformed
        self.pos += 1
        return self.data[self.pos - 1]


# --- Mode 0: attributes --------------------------------------------------


def decode_attributes(data: bytes, count: int, stride: int) -> bytes:
    """``count`` elements of ``stride`` bytes, from a buffer of version 1 of
    the attribute codec."""
    data = bytes(data)
    if not data or data[0] != 0xA1:
        raise Malformed
    channels = stride // 4
    # The tail: zero padding, then the baseline element and the channel modes,
    # together at least 24 bytes.
    tail_size = max(stride + channels, 24)
    if len(data) < 1 + tail_size:
        raise Malformed
    end = len(data) - tail_size
    baseline = data[len(data) - stride - channels : len(data) - channels]
    modes = data[len(data) - channels :]
    for mode in modes:
        if mode & 0x0F > 2 or (mode & 0x0F < 2 and mode >> 4):
            raise Malformed

    cursor = _Cursor(data, 1, end)
    out = bytearray(count * stride)
    previous = bytearray(baseline)
    block_size = min((8192 // stride) & ~15, 256)
    done = 0
    while done < count:
        elements = min(count - done, block_size)
        groups = (elements + 15) // 16
        control = cursor.take(channels)
        # deltas[k][i]: byte k of the delta of element i of the block.
        deltas = [
            _data_block(cursor, (control[k >> 2] >> ((k & 3) * 2)) & 3, groups, elements) for k in range(stride)
        ]
        for i in range(elements):
            for c in range(channels):
                _undelta(previous, 4 * c, [deltas[4 * c + j][i] for j in range(4)], modes[c])
            out[(done + i) * stride : (done + i + 1) * stride] = previous
        done += elements
    if cursor.pos != end:
        raise Malformed
    return bytes(out)


# Bits per delta for each delta encoding mode, by control mode; 8 means the
# 16 deltas are stored as bytes.
_WIDTHS = {0: (0, 1, 2, 4), 1: (1, 2, 4, 8)}


def _data_block(cursor: _Cursor, control: int, groups: int, elements: int) -> bytes:
    """One byte position's deltas, for every element of the block."""
    if control == 2:
        return bytes(16 * groups)
    if control == 3:
        # Literal: one delta byte per element, not rounded up to the group.
        return cursor.take(elements)
    header = cursor.take((groups + 3) // 4)
    out = bytearray()
    for g in range(groups):
        width = _WIDTHS[control][(header[g >> 2] >> ((g & 3) * 2)) & 3]
        if width == 0:
            out += bytes(16)
        elif width == 8:
            out += cursor.take(16)
        else:
            out += bytes(_sentinel_group(cursor, width))
    return bytes(out)


def _sentinel_group(cursor: _Cursor, width: int) -> list[int]:
    packed = cursor.take(2 * width)
    per_byte = 8 // width
    mask = (1 << width) - 1
    values = []
    for i in range(16):
        byte = packed[i // per_byte]
        slot = i % per_byte
        # 1-bit deltas go from the least significant bit up; 2- and 4-bit
        # deltas from the most significant bit down.
        shift = slot if width == 1 else 8 - width * (slot + 1)
        values.append((byte >> shift) & mask)
    # A delta with every bit set is stored as a whole byte after the group.
    return [cursor.byte() if v == mask else v for v in values]


def _undelta(previous: bytearray, at: int, delta: list[int], mode: int) -> None:
    """Applies one channel's 4 delta bytes to the previous element, in place."""
    kind = mode & 0x0F
    if kind == 0:  # byte deltas, zigzag-encoded
        for j in range(4):
            previous[at + j] = (previous[at + j] + _unzigzag(delta[j], 8)) & 0xFF
    elif kind == 1:  # 2-byte deltas, zigzag-encoded
        for j in (0, 2):
            old = previous[at + j] | previous[at + j + 1] << 8
            new = (old + _unzigzag(delta[j] | delta[j + 1] << 8, 16)) & 0xFFFF
            previous[at + j], previous[at + j + 1] = new & 0xFF, new >> 8
    else:  # 4-byte XOR deltas, rotated
        rotation = mode >> 4
        value = int.from_bytes(bytes(delta), "little")
        xor = ((value >> rotation) | (value << ((32 - rotation) & 31))) & _MASK32
        old = int.from_bytes(previous[at : at + 4], "little")
        previous[at : at + 4] = (old ^ xor).to_bytes(4, "little")


def _unzigzag(value: int, bits: int) -> int:
    mask = (1 << bits) - 1
    return (~(value >> 1) & mask) if value & 1 else value >> 1


# --- Mode 1: triangles ---------------------------------------------------


class _Fifo:
    """A 16-entry FIFO; reading an entry never written is malformed."""

    def __init__(self) -> None:
        self.entries: list = [None] * 16
        self.written = 0

    def push(self, value) -> None:
        self.entries[self.written & 15] = value
        self.written += 1

    def get(self, index: int):
        # 0 is the most recently added entry.
        if index >= min(self.written, 16):
            raise Malformed
        return self.entries[(self.written - 1 - index) & 15]


def decode_triangles(data: bytes, triangle_count: int) -> array:
    """The ``3 * triangle_count`` indices of a buffer of the triangle codec,
    version 1, as ``u32`` values."""
    data = bytes(data)
    if not data or data[0] != 0xE1:
        raise Malformed
    if triangle_count > len(data) - 17:
        raise Malformed
    codes = data[1 : 1 + triangle_count]
    aux = data[len(data) - 16 :]
    if aux[14] or aux[15] or any(b >> 4 == 0xF or b & 0x0F == 0xF for b in aux):
        raise Malformed
    extra = _Cursor(data, 1 + triangle_count, len(data) - 16)

    edges = _Fifo()
    vertices = _Fifo()
    state = {"next": 0, "last": 0}
    out = array("I")

    def new() -> int:
        value = state["next"]
        state["next"] = (value + 1) & _MASK32
        return value

    def index() -> int:
        # varint-7: 1 to 5 bytes, the high bit set on every byte but the last.
        value = 0
        for i in range(5):
            byte = extra.byte()
            value |= (byte & 0x7F) << (7 * i)
            if not byte & 0x80:
                break
        else:
            raise Malformed
        value &= _MASK32
        delta = (~(value >> 1) & _MASK32) if value & 1 else value >> 1
        state["last"] = (state["last"] + delta) & _MASK32
        return state["last"]

    for code in codes:
        high, low = code >> 4, code & 0x0F
        if high < 0xF:
            a, b = edges.get(high)
            if low == 0:
                c = new()
                vertices.push(c)
            elif low < 0xD:
                c = vertices.get(low)
            elif low < 0xF:
                c = (state["last"] + (1 if low == 0xE else -1)) & _MASK32
                state["last"] = c
                vertices.push(c)
            else:
                c = index()
                vertices.push(c)
            edges.push((c, b))
            edges.push((a, c))
        else:
            if low < 0xE:
                z, w = aux[low] >> 4, aux[low] & 0x0F
                a = new()
                b = new() if z == 0 else vertices.get(z - 1)
                c = new() if w == 0 else vertices.get(w - 1)
            else:
                zw = extra.byte()
                if zw == 0:
                    state["next"] = 0
                z, w = zw >> 4, zw & 0x0F
                a = new() if low == 0xE else index()
                b = new() if z == 0 else vertices.get(z - 1) if z < 0xF else index()
                c = new() if w == 0 else vertices.get(w - 1) if w < 0xF else index()
            edges.push((b, a))
            edges.push((c, b))
            edges.push((a, c))
            vertices.push(a)
            if z == 0 or z == 0xF:
                vertices.push(b)
            if w == 0 or w == 0xF:
                vertices.push(c)
        out.extend((a, b, c))
    if extra.pos != extra.end:
        raise Malformed
    return out
