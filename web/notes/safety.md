---
title: Safe by arithmetic
status: stable
updated: 2026-11-11
---

Lengths come from the file, so a damaged one can be enormous: one flipped bit in a length asks for
$2^{63}$ bytes. No count in a `.bread` file can make a reader ask for more memory than the file
could fill.

The rule is one inequality, checked before allocating anything. Items that take at least $k$ bytes
each cannot number more than the bytes left, divided by $k$:

$$n \le \left\lfloor \frac{\text{bytes left}}{k} \right\rfloor$$

Every structure in the format comes with its $k$. The meshes add three bounds of their own, with
$L$ the bytes of the buffer, $n$ vertices and $t$ triangles:

| What | Bound | Why |
|---|---|---|
| vertices, attribute codec | $3n < 256\,L$ | every block of up to 256 vertices starts with a 3-byte header |
| triangles, triangle codec | $t \le L - 17$ | at least one byte per triangle, past the first byte and a 16-byte tail |
| vertices, rANS coding | $n \le 3t$ | a small payload cannot ask for a large mesh |

The same goes for the container. Stored bytes are read as they arrive, or checked against what is
left of the file. Payloads are decompressed as a stream that stops the moment it passes the raw
length, instead of allocating the raw length up front; the content size a Zstandard frame records
is just as untrusted.

So memory grows with what a file really holds once decompressed, never with what a damaged length
claims. A file crafted on purpose cannot make the reference reader ask for huge amounts of memory,
or hang it. A reader may still set limits of its own, and should then say the file is too large for
it, not that it is damaged.

Two conformance files test exactly this: one asks for a payload of $2^{63}$ bytes, the other for a
mesh of $2^{32} - 1$ vertices, 48 GiB. A reader that allocates before checking runs out of memory on
them instead of refusing them.
