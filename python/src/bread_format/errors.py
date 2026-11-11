# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Why a file is refused."""

from __future__ import annotations

#: What each reason code of conformance/README.md means.
MESSAGES = {
    "not-bread": "not a .bread file",
    "version": "written in a newer version of the format",
    "cut-short": "the file is cut short",
    "crc": "a chunk is damaged: its bytes do not match their checksum",
    "bad-payload": "a chunk does not decompress to its recorded length",
    "chunk-count": "a chunk is missing, or there are more of it than the format allows",
    "malformed": "a chunk is malformed",
    "no-such-vertex": "a triangle names a vertex its mesh does not have",
}


class BreadError(Exception):
    """A file the reader refused.

    ``reason`` is one of the reason codes of conformance/README.md, and
    ``chunk`` the tag of the chunk it is about, such as ``"MESH"``, or None
    when it is about the whole file.
    """

    def __init__(self, reason: str, chunk: str | None = None) -> None:
        message = MESSAGES.get(reason, reason)
        super().__init__(message if chunk is None else f"{message} ({chunk} chunk)")
        self.reason = reason
        self.chunk = chunk


class Malformed(Exception):
    """A payload that is malformed (SPEC 3.3). The caller knows the chunk."""


class NoSuchVertex(Exception):
    """A triangle that names a vertex its mesh does not have (SPEC 6.4)."""
