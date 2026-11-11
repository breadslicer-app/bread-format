# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The container (SPEC 4): the header, and chunks one after another."""

from __future__ import annotations

import struct
import zlib
from compression import zstd
from dataclasses import dataclass

from .errors import BreadError

MAGIC = b"BREAD\x00\r\n"
CONTAINER_VERSION = 1

_CHUNK_HEADER = struct.Struct("<4sHQQI")  # tag, version, raw, stored, CRC: 26 bytes

# How much is decompressed at a time: memory grows with what a payload really
# holds, never with the raw length it claims (SPEC 4.5).
_STEP = 1 << 20


@dataclass(frozen=True)
class RawChunk:
    """A chunk as the container holds it, before it is decompressed."""

    tag: bytes
    version: int
    raw_length: int
    stored: memoryview

    @property
    def name(self) -> str:
        """The tag as text, for messages: four ASCII letters for any official tag."""
        return self.tag.decode("latin-1")


def split_chunks(data: bytes | memoryview) -> list[RawChunk]:
    """Checks the header and every chunk's CRC, and returns the chunks in file
    order, still compressed (SPEC 4.1, 4.2, 4.4)."""
    data = memoryview(data)
    if len(data) < len(MAGIC) or data[: len(MAGIC)] != MAGIC:
        raise BreadError("not-bread")
    if len(data) < 10:
        raise BreadError("cut-short")
    (version,) = struct.unpack_from("<H", data, 8)
    if version != CONTAINER_VERSION:
        # Versions start at 1 (SPEC 3.4), so 0 is no version this reader knows
        # either.
        raise BreadError("version")
    chunks = []
    pos = 10
    while pos < len(data):
        if len(data) - pos < _CHUNK_HEADER.size:
            raise BreadError("cut-short")
        tag, chunk_version, raw_length, stored_length, crc = _CHUNK_HEADER.unpack_from(data, pos)
        pos += _CHUNK_HEADER.size
        # Checked against what is left of the file before it is read (SPEC 4.5).
        if stored_length > len(data) - pos:
            raise BreadError("cut-short")
        stored = data[pos : pos + stored_length]
        pos += stored_length
        chunk = RawChunk(tag, chunk_version, raw_length, stored)
        if zlib.crc32(stored) != crc:
            raise BreadError("crc", chunk.name)
        chunks.append(chunk)
    return chunks


def decompress(chunk: RawChunk) -> bytes:
    """The chunk's payload: one Zstandard frame that decompresses to exactly
    the raw length (SPEC 4.3), decompressed as a stream that stops as soon as
    it passes the raw length (SPEC 4.5)."""
    decompressor = zstd.ZstdDecompressor()
    out = bytearray()
    data = bytes(chunk.stored)
    try:
        while True:
            want = min(chunk.raw_length + 1 - len(out), _STEP)
            out += decompressor.decompress(data, max_length=want)
            data = b""
            if len(out) > chunk.raw_length or decompressor.eof or decompressor.needs_input:
                break
    except zstd.ZstdError:
        raise BreadError("bad-payload", chunk.name) from None
    if len(out) != chunk.raw_length or not decompressor.eof or decompressor.unused_data:
        raise BreadError("bad-payload", chunk.name)
    return bytes(out)
