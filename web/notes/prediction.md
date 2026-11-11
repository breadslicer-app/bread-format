---
title: Predicting vertices
status: stable
updated: 2026-11-11
---

Most of a mesh is coordinates, and most coordinates sit almost where their neighbours say they
should. `MESH` version 2, with vertex coding 1 or 2, stores only how far off they are.

Vertices are decoded in increasing number, so vertex $v$ is predicted from vertices $0 \ldots v-1$
and the triangles, turned by their corners. Every triangle that has $v$ as a corner is visited once,
in increasing order. Let $a$ and $b$ be the two corners after $v$ in that triangle, and
$\mathbf{A}$, $\mathbf{B}$ their coordinates. Three sums, each with a count, collect guesses:

1. **para**, when $a, b < v$ and the first *other* triangle across edge $ab$ has a known third
   corner $c$: the parallelogram $(\mathbf{A} + \mathbf{B}) - \mathbf{C}$. The new triangle $ABv$
   mirrors the known triangle $ABC$ across their shared edge;
2. **mid**, when $a, b < v$: the midpoint $0.5\,(\mathbf{A} + \mathbf{B})$;
3. **near**, every known corner alone: $\mathbf{A}$ if $a < v$, $\mathbf{B}$ if $b < v$.

The prediction is the average of the first sum that got anything, in that order. If none did, it is
the previous vertex, or the origin for vertex 0:

$$\hat{p}_v = \begin{cases} \dfrac{1}{n_\text{para}} \sum \big((\mathbf{A} + \mathbf{B}) - \mathbf{C}\big) & n_\text{para} > 0 \\[2ex] \dfrac{1}{n_\text{mid}} \sum 0.5\,(\mathbf{A} + \mathbf{B}) & n_\text{para} = 0 < n_\text{mid} \\[2ex] \dfrac{1}{n_\text{near}} \sum \mathbf{N} & \text{only near} \\[2ex] x_{v-1} \text{ (or } \mathbf{0} \text{ for } v = 0) & \text{nothing known} \end{cases}$$

Only the *first* other triangle across an edge counts. That keeps the work per vertex bounded, and
lets a reader find it with a sorted list of edges, in linear time.

**Exact on every machine.** Every value is an IEEE 754 binary64. Every operation rounds to nearest,
ties to even, in the order written, with no fused multiply-add. The average is rounded to `f32` once,
at the end, and a component that is not finite becomes $+0$. That is what lets a writer on Windows
and a reader in a browser compute the same prediction, bit for bit.

## The residual

The difference is not taken between floats, which could round, but between their bits.
$\operatorname{ord}$ maps the 32 bits $u$ of an `f32` to an unsigned integer that orders the way
the values do. The gap between the two is then folded so that small gaps, either way, become small
numbers:

$$\operatorname{ord}(u) = \begin{cases} \lnot u & \text{sign bit set} \\ u \mathbin{|} 2^{31} & \text{otherwise} \end{cases}$$

$$d = \big(\operatorname{ord}(x) - \operatorname{ord}(\hat p)\big) \bmod 2^{32}, \qquad r = \operatorname{zigzag}(d) = \begin{cases} 2d & d \ge 0 \\ -2d - 1 & d < 0 \end{cases}$$

with $d$ read as a two's complement `i32`. A reader undoes both steps: $x$ is the `f32` whose
$\operatorname{ord}$ is $(\operatorname{ord}(\hat p) + d) \bmod 2^{32}$. Every residual gives back
some `f32`, NaN payloads and $-0$ included, so the coordinates come back bit for bit.

Worked example: $x = 10.37$ and $\hat p = 10.3651$, both rounded to `f32` first. Their bits are
`0x4125EB85` and `0x4125D773`, their ords `0xC125EB85` and `0xC125D773`, so $d = 5138$ and
$r = 10\,276$, a residual of 14 bits.
