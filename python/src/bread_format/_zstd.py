# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Zstandard decompression (SPEC 4.3): from the standard library on Python 3.14
and later (``compression.zstd``), and from the ``zstandard`` package before it.
Both decompress one frame as a stream that stops as soon as it passes a limit,
so memory grows with what a payload really holds, never with what it claims
(SPEC 4.5)."""

from __future__ import annotations

try:
    from compression import zstd as _stdlib
except ImportError:  # Python before 3.14
    _stdlib = None

try:
    import zstandard as _package
except ImportError:  # not needed on Python 3.14 and later
    _package = None

# How much the standard library decompresses at a time.
_STEP = 1 << 20

# How much compressed input the zstandard package is given at a time, since it
# cannot be told to stop after a number of bytes. A block holds at most 128 KiB
# and takes at least 4 bytes, so 256 bytes of input come out as 8 MiB at most:
# that bounds how far past the limit it can go.
_FEED = 256


class Failed(Exception):
    """The data is not a Zstandard frame that can be decompressed."""


def inflate_stdlib(data: bytes, limit: int) -> tuple[bytes, bool]:
    """``inflate`` with ``compression.zstd``."""
    decompressor = _stdlib.ZstdDecompressor()
    out = bytearray()
    try:
        while True:
            want = min(limit + 1 - len(out), _STEP)
            out += decompressor.decompress(data, max_length=want)
            data = b""
            if len(out) > limit or decompressor.eof or decompressor.needs_input:
                break
    except _stdlib.ZstdError:
        raise Failed from None
    return bytes(out), decompressor.eof and not decompressor.unused_data


def inflate_package(data: bytes, limit: int) -> tuple[bytes, bool]:
    """``inflate`` with the ``zstandard`` package."""
    decompressor = _package.ZstdDecompressor().decompressobj()
    out = bytearray()
    pos = 0
    try:
        while pos < len(data) and len(out) <= limit and not decompressor.eof:
            out += decompressor.decompress(data[pos : pos + _FEED])
            pos += _FEED
    except _package.ZstdError:
        raise Failed from None
    whole = decompressor.eof and not decompressor.unused_data and pos >= len(data)
    return bytes(out), whole


if _stdlib is not None:
    _backend = inflate_stdlib
elif _package is not None:
    _backend = inflate_package
else:
    raise ImportError("bread-format needs Python 3.14 or later, or the zstandard package")


def inflate(data: bytes, limit: int) -> tuple[bytes, bool]:
    """Decompresses the Zstandard frame that ``data`` starts with, and stops as
    soon as more than ``limit`` bytes are out. Returns what came out, and
    whether the frame ended, with nothing after it in ``data``. Raises
    ``Failed`` when ``data`` is not a frame that decompresses."""
    return _backend(data, limit)
