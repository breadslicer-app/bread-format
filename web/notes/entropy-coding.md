---
title: Entropy coding the residuals
status: stable
updated: 2026-11-11
---

Residuals are small and lopsided, which is exactly what an entropy coder eats. `MESH` version 2,
vertex coding 1, codes them with static rANS, in 102 contexts.

**Symbols.** A residual $r$ can take any of $2^{32}$ values, too many for a table. So it is split:
its bit length and its top bits become one of 132 symbols, and the bits below go out raw. With
$L = \operatorname{bitlen}(r)$:

$$s = \begin{cases} 4L & L \le 1 \\ 4L + \Big(\big((r \gg m) \bmod 2^h\big) \ll (2 - h)\Big) & L \ge 2 \end{cases} \qquad n = L - 1,\quad h = \min(n, 2),\quad m = n - h$$

and the raw bits are the $m$ lowest bits of $r$. A reader turns a symbol back into

$$r = 2^n + \Big(\big((s \bmod 4) \gg (2 - h)\big) \ll m\Big) + \text{raw bits}$$

and refuses a symbol no residual gives: one with $L \le 1$ and $s \bmod 4 \ne 0$, or one whose
lowest $2 - h$ bits are not zero.

Worked example: $r = 182 = 10110110_2$. Then $L = 8$, $n = 7$, $h = 2$, $m = 5$: the symbol is
$32 + 1 = 33$, and the raw bits are $10110_2$.

**Contexts.** Residuals are coded vertex by vertex, x then y then z, each in a context that says how
big its neighbours were, because big residuals come in clumps. Component $c$ (0 x, 1 y, 2 z) uses
context $34c + k$, where $k$ is:

- for x: the largest bit length among the three residuals of the vertex before (0 for vertex 0);
- for y: the bit length of this vertex's x residual;
- for z: the bit lengths of its x and y residuals, added, halved and rounded up.

**Tables.** A bit stream. For each context, one bit says whether it has a table. A table gives each
of the 132 symbols one bit, then a frequency minus 1 in 12 bits when the bit is set. Each table's
frequencies add up to exactly $2^{12} = 4096$, and a symbol owns the slots that start at the sum of
the frequencies before it.

**Decoding one symbol.** The state $x$ is 32 bits:

$$\textit{slot} = x \bmod 4096, \qquad x \leftarrow f_s \cdot (x \gg 12) + \textit{slot} - \textit{start}_s, \qquad \text{while } x < 2^{23}:\ x \leftarrow 256\,x + \text{next byte}$$

where $s$ is the symbol whose slots hold *slot*.

**Encoding, in reverse.** The writer starts from $x = 2^{23}$ and codes the symbols last to first.
For each one, while $x \ge 2^{19} f_s$ it writes $x$'s lowest byte and shifts $x$ right by 8, then

$$x \leftarrow \left\lfloor \frac{x}{f_s} \right\rfloor \cdot 4096 + (x \bmod f_s) + \textit{start}_s$$

At the end it writes $x$'s four bytes and reverses every byte it wrote, so the reader goes first to
last and starts from the first four bytes, big-endian. A stream must end with no bytes left and the
state back at exactly $2^{23}$: a free check that the whole stream was read.

**The other codings.** Coding 0 stores the vertices with the meshopt attribute codec, as in version
1. Coding 2 stores the residuals as 12 byte planes: plane $4c + i$ holds byte $i$ of every residual
of component $c$, and the chunk's Zstandard squeezes them. A writer may use any coding the mesh
meets the rules of. The reference writer keeps whichever stores the mesh smallest.
