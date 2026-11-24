# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Both ways of decompressing, the standard library's and the zstandard
package's, each where it is installed: the same answers, and neither goes far
past the limit (SPEC 4.5)."""

from __future__ import annotations

import os
import unittest

from bread_format import _zstd

BACKENDS = {}
if _zstd._stdlib is not None:
    BACKENDS["stdlib"] = _zstd.inflate_stdlib
if _zstd._package is not None:
    BACKENDS["package"] = _zstd.inflate_package


def compress(data: bytes) -> bytes:
    if _zstd._stdlib is not None:
        return _zstd._stdlib.compress(data)
    return _zstd._package.ZstdCompressor().compress(data)


class Inflate(unittest.TestCase):
    def each(self):
        for name, inflate in BACKENDS.items():
            with self.subTest(backend=name):
                yield inflate

    def test_one_frame(self):
        data = os.urandom(5000) + bytes(300_000)
        frame = compress(data)
        for inflate in self.each():
            self.assertEqual(inflate(frame, len(data)), (data, True))

    def test_empty(self):
        frame = compress(b"")
        for inflate in self.each():
            self.assertEqual(inflate(frame, 0), (b"", True))

    def test_bytes_after_the_frame(self):
        frame = compress(b"bread" * 100)
        for inflate in self.each():
            for extra in (b"\x00", compress(b"more")):
                out, whole = inflate(frame + extra, 500)
                self.assertEqual(out, b"bread" * 100)
                self.assertFalse(whole)

    def test_cut_short(self):
        frame = compress(os.urandom(2000))
        for inflate in self.each():
            try:
                out, whole = inflate(frame[:-10], 2000)
            except _zstd.Failed:
                continue
            self.assertFalse(whole)

    def test_not_a_frame(self):
        for inflate in self.each():
            with self.assertRaises(_zstd.Failed):
                inflate(b"\x28\xb5\x2f\xfd" + b"\xff" * 40, 100)

    def test_stops_past_the_limit(self):
        # 256 MiB of zeros in a few kilobytes: a payload that claims 1000
        # bytes must not be decompressed in full.
        frame = compress(bytes(256 << 20))
        for inflate in self.each():
            out, whole = inflate(frame, 1000)
            self.assertGreater(len(out), 1000)
            self.assertLessEqual(len(out), 16 << 20)
            self.assertFalse(whole)

    def test_both_agree(self):
        if len(BACKENDS) < 2:
            self.skipTest("only one way to decompress is installed")
        data = os.urandom(70_000) * 3
        frame = compress(data)
        for limit in (0, 1, 1000, len(data) - 1, len(data), len(data) + 1):
            answers = []
            for name in ("stdlib", "package"):
                out, whole = BACKENDS[name](frame, limit)
                # What decides a payload (SPEC 4.3): exactly the raw length,
                # and the frame ends there. Past the limit, how far each one
                # went before stopping does not matter.
                answers.append((len(out) == limit and whole, out[: limit + 1]))
            self.assertEqual(answers[0], answers[1], limit)


class Conformance(unittest.TestCase):
    """Every chunk of every conformance file, decompressed each way: the same
    payload, or the same refusal."""

    def test_every_chunk(self):
        from pathlib import Path
        from unittest import mock

        from bread_format import BreadError
        from bread_format.container import decompress, split_chunks

        root = Path(__file__).resolve().parents[2] / "conformance"
        files = sorted(root.glob("*/*.bread"))
        self.assertGreater(len(files), 40)
        for path in files:
            try:
                chunks = split_chunks(path.read_bytes())
            except BreadError:
                continue
            for chunk in chunks:
                answers = {}
                for name, inflate in BACKENDS.items():
                    with mock.patch.object(_zstd, "_backend", inflate):
                        try:
                            answers[name] = decompress(chunk)
                        except BreadError as e:
                            answers[name] = e.args
                self.assertEqual(len(set(map(repr, answers.values()))), 1, (path.name, chunk.name))


if __name__ == "__main__":
    unittest.main()
