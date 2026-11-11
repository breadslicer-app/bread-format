# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Reading payload fields (SPEC 3.1 to 3.3) and bit streams (SPEC 6.6)."""

from __future__ import annotations

import struct
from dataclasses import dataclass

from .errors import Malformed

_U16 = struct.Struct("<H")
_U32 = struct.Struct("<I")
_U64 = struct.Struct("<Q")
_F64 = struct.Struct("<d")


@dataclass(frozen=True)
class Meta:
    """A name and a comment, each a string or None (SPEC 5.1, 6.2)."""

    name: str | None
    comment: str | None


class Fields:
    """Reads the fields of one payload, in order.

    Every read past the end raises Malformed, and so does ``end`` when bytes
    are left: a payload that is cut short or padded is malformed (SPEC 3.3).
    """

    def __init__(self, data: bytes | memoryview) -> None:
        self._data = memoryview(data)
        self._pos = 0

    def left(self) -> int:
        return len(self._data) - self._pos

    def take(self, n: int) -> memoryview:
        if n > self.left():
            raise Malformed
        out = self._data[self._pos : self._pos + n]
        self._pos += n
        return out

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return _U16.unpack(self.take(2))[0]

    def u32(self) -> int:
        return _U32.unpack(self.take(4))[0]

    def u64(self) -> int:
        return _U64.unpack(self.take(8))[0]

    def f64(self) -> float:
        return _F64.unpack(self.take(8))[0]

    def flag(self) -> bool:
        value = self.u8()
        if value > 1:
            raise Malformed
        return value == 1

    def count(self, at_least: int) -> int:
        """A count of items that take at least ``at_least`` bytes each,
        checked against the bytes left before anything is allocated for it."""
        n = self.u32()
        if n > self.left() // at_least:
            raise Malformed
        return n

    def byte_string(self) -> memoryview:
        return self.take(self.u32())

    def text(self) -> str:
        try:
            return str(self.byte_string(), "utf-8")
        except UnicodeDecodeError:
            raise Malformed from None

    def optional_text(self) -> str | None:
        return self.text() if self.flag() else None

    def meta(self) -> Meta | None:
        """A *has metadata* flag, then a name and a comment when it is set."""
        if not self.flag():
            return None
        return Meta(self.optional_text(), self.optional_text())

    def end(self) -> None:
        if self.left():
            raise Malformed


class Bits:
    """Reads a bit stream (SPEC 6.6): values packed least significant bit
    first. ``end`` checks that the stream is exactly as long as the bits read
    need, with the unused bits of its last byte at 0."""

    def __init__(self, data: bytes | memoryview) -> None:
        self._data = bytes(data)
        self._pos = 0

    def read(self, n: int) -> int:
        value = 0
        got = 0
        while got < n:
            index = self._pos >> 3
            if index >= len(self._data):
                raise Malformed
            shift = self._pos & 7
            width = min(8 - shift, n - got)
            value |= ((self._data[index] >> shift) & ((1 << width) - 1)) << got
            got += width
            self._pos += width
        return value

    def end(self) -> None:
        if len(self._data) != (self._pos + 7) >> 3:
            raise Malformed
        if self._pos & 7 and self._data[-1] >> (self._pos & 7):
            raise Malformed
