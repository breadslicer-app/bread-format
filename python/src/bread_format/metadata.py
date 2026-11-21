# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""The META chunk: who made a file, how it may be used, and lasting ids for
its parts (SPEC 12). It never makes a file unreadable."""

from __future__ import annotations

import re
from dataclasses import dataclass

from ._bytes import Fields
from .errors import Malformed

#: The registered keys whose value is UTF-8 text (SPEC 12.1).
TEXT_KEYS = ("title", "description", "designer", "copyright", "license", "source", "created", "generator")

#: The subject that stands for the whole file in a payload.
FILE = 0xFFFFFFFF

_KEY = re.compile(rb"[a-z][a-z0-9._-]{0,63}")


@dataclass(frozen=True)
class MetaEntry:
    """A META entry. ``node`` is the node index it is about, or None for the
    whole file."""

    node: int | None
    key: str
    value: bytes

    @property
    def text(self) -> str | None:
        """The value of a registered text key."""
        return self.value.decode("utf-8") if self.key in TEXT_KEYS else None

    @property
    def uuid(self) -> bytes | None:
        """The 16 bytes of a ``uuid``."""
        return self.value if self.key == "uuid" else None


def _fits(key: bytes, value: bytes) -> bool:
    if not _KEY.fullmatch(key):
        return False
    if b"." in key:
        return True  # a program's own key: any value
    if key == b"uuid":
        return len(value) == 16
    if key.decode() in TEXT_KEYS:
        try:
            value.decode("utf-8")
        except UnicodeDecodeError:
            return False
        return True
    return False  # reserved


def read_metadata(payload: bytes, node_count: int) -> list[MetaEntry]:
    """The entries of a META payload of version 1 that a reader keeps: none
    when the payload is malformed, and none that break a rule (SPEC 12.2)."""
    try:
        f = Fields(payload)
        raw = [(f.u32(), bytes(f.byte_string()), bytes(f.byte_string())) for _ in range(f.count(12))]
        f.end()
    except Malformed:
        return []
    seen = set()
    entries = []
    for subject, key, value in raw:
        if (subject, key) in seen:
            continue  # the second of two, whatever the first holds
        seen.add((subject, key))
        if subject != FILE and subject >= node_count:
            continue
        if not _fits(key, value):
            continue
        entries.append(MetaEntry(None if subject == FILE else subject, key.decode(), value))
    return entries
