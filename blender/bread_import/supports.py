# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: GPL-3.0-or-later

"""Builds the mesh of a set of supports from its document.

A Breadslicer supports document (generator "df", SPEC 7.3) already says where
every piece goes and how big it is: the bases on the plate, the trunks and
branches as chains of joints, the tips where they touch the model, the knots
where one piece leaves another, and the braces between knots. Nothing here
decides where supports go: it only gives each piece a shape.

- a base is a disk, with a cone that joins it to its trunk; on a raft, a
  small foot instead: a pad a little wider than a line of a line raft and a
  cone up to the trunk, since the raft already holds the supports down;
- a stretch between two joints is a cylinder, and each joint and knot a ball;
- a tip is a thin disk on the model and a cone from it to the support;
- a brace is a cylinder from knot to knot;
- the raft, when there is one, is a plate under the bases, sloped underneath,
  with a low wall along its edge that has gaps for resin to drain.

The pieces overlap and are not merged, as a slicer expects them. Everything is
built with numpy, in batches, so thousands of supports take a fraction of a
second. No Blender here, so it can be tested without it."""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, field

import numpy as np

GENERATOR = "df"
# The document versions this builder reads.
VERSIONS = (1, 2)

# Sides of every cylinder and ball, for each level of detail.
DETAIL_SIDES = {"LOW": 8, "MEDIUM": 12, "HIGH": 24}


class SupportsError(Exception):
    pass


def _vec(d: dict | None) -> np.ndarray | None:
    if not isinstance(d, dict):
        return None
    try:
        return np.array([float(d["x"]), float(d["y"]), float(d["z"])])
    except (KeyError, TypeError, ValueError):
        return None


def _num(d: dict | None, key: str, default: float = 0.0) -> float:
    if not isinstance(d, dict):
        return default
    v = d.get(key)
    try:
        v = float(v)
    except (TypeError, ValueError):
        return default
    return v if math.isfinite(v) else default


def _unit(v: np.ndarray) -> np.ndarray | None:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else None


def _rotate(q: dict | None, v: np.ndarray) -> np.ndarray:
    """v turned by the unit quaternion q, as {x, y, z, w}."""
    if not isinstance(q, dict):
        return v
    x, y, z, w = (_num(q, k, d) for k, d in (("x", 0), ("y", 0), ("z", 0), ("w", 1)))
    if x == 0 and y == 0 and v[0] == 0 and v[1] == 0:
        return v  # a turn about z leaves z alone
    u = np.array([x, y, z])
    return v + 2 * np.cross(u, np.cross(u, v) + w * v)


@dataclass
class Shapes:
    """Cylinders (two ends and two radii, so cones too) and balls."""

    cylinders: list[tuple] = field(default_factory=list)
    balls: list[tuple] = field(default_factory=list)
    # The bases, the raft's thickness, and whether it has a wall.
    raft: tuple[list, float, bool] | None = None

    def cylinder(self, p0, p1, r0, r1=None):
        if p0 is None or p1 is None:
            return
        r1 = r0 if r1 is None else r1
        if np.linalg.norm(p1 - p0) < 1e-9 or (r0 <= 0 and r1 <= 0):
            return
        self.cylinders.append((p0, p1, max(r0, 0.0), max(r1, 0.0)))

    def ball(self, centre, radius):
        if centre is not None and radius > 0:
            self.balls.append((centre, radius))


def _cylinder_mesh(c: list[tuple], sides: int):
    n = len(c)
    p0 = np.array([x[0] for x in c])
    p1 = np.array([x[1] for x in c])
    r0 = np.array([x[2] for x in c])
    r1 = np.array([x[3] for x in c])
    axis = p1 - p0
    axis /= np.linalg.norm(axis, axis=1)[:, None]
    helper = np.where(np.abs(axis[:, :1]) < 0.9, [[1.0, 0, 0]], [[0, 1.0, 0]])
    u = np.cross(axis, helper)
    u /= np.linalg.norm(u, axis=1)[:, None]
    v = np.cross(axis, u)
    t = np.arange(sides) * (2 * math.pi / sides)
    dirs = np.cos(t)[None, :, None] * u[:, None, :] + np.sin(t)[None, :, None] * v[:, None, :]
    ring0 = p0[:, None, :] + r0[:, None, None] * dirs
    ring1 = p1[:, None, :] + r1[:, None, None] * dirs
    verts = np.concatenate([ring0, ring1, p0[:, None, :], p1[:, None, :]], axis=1)
    i = np.arange(sides)
    j = (i + 1) % sides
    a0, a1, b0, b1 = i, j, sides + i, sides + j
    c0 = np.full(sides, 2 * sides)
    c1 = np.full(sides, 2 * sides + 1)
    tris = np.concatenate([
        np.stack([a0, a1, b1], 1),
        np.stack([a0, b1, b0], 1),
        np.stack([c1, b0, b1], 1),
        np.stack([c0, a1, a0], 1),
    ])
    per = 2 * sides + 2
    tris = tris[None, :, :] + (np.arange(n) * per)[:, None, None]
    return verts.reshape(-1, 3), tris.reshape(-1, 3)


def _ball_template(sides: int):
    rings = max(sides // 2, 3)
    verts = [(0.0, 0.0, 1.0)]
    for k in range(1, rings):
        phi = math.pi * k / rings
        for s in range(sides):
            th = 2 * math.pi * s / sides
            verts.append((math.sin(phi) * math.cos(th), math.sin(phi) * math.sin(th), math.cos(phi)))
    verts.append((0.0, 0.0, -1.0))
    bottom = len(verts) - 1
    tris = []
    ring = lambda k, s: 1 + k * sides + (s % sides)  # noqa: E731
    for s in range(sides):
        tris.append((0, ring(0, s), ring(0, s + 1)))
    for k in range(rings - 2):
        for s in range(sides):
            a_i, a_j, b_i, b_j = ring(k, s), ring(k, s + 1), ring(k + 1, s), ring(k + 1, s + 1)
            tris.append((a_i, b_i, b_j))
            tris.append((a_i, b_j, a_j))
    for s in range(sides):
        tris.append((bottom, ring(rings - 2, s + 1), ring(rings - 2, s)))
    return np.array(verts), np.array(tris)


def _ball_mesh(b: list[tuple], sides: int):
    tv, tt = _ball_template(sides)
    centres = np.array([x[0] for x in b])
    radii = np.array([x[1] for x in b])
    verts = centres[:, None, :] + radii[:, None, None] * tv[None, :, :]
    tris = tt[None, :, :] + (np.arange(len(b)) * len(tv))[:, None, None]
    return verts.reshape(-1, 3), tris.reshape(-1, 3)


def _hull(points: np.ndarray) -> np.ndarray:
    """The convex hull of 2D points, counter-clockwise."""
    pts = sorted(set(map(tuple, np.round(points, 9))))
    if len(pts) < 3:
        return np.array(pts)

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return np.array(lower[:-1] + upper[:-1])


# The raft: a plate under the bases, its edge sloped at 45° underneath, and a
# low wall along its top edge with gaps every few millimetres, so resin
# drains.
RAFT_WALL_HEIGHT = 0.35
RAFT_WALL_THICKNESS = 0.5
RAFT_WALL_GAP = 1.5
RAFT_WALL_SPACING = 5.0
# How tall a line of a line raft is, and how wide.
RAFT_LINE_HEIGHT = 0.6
RAFT_LINE_WIDTH = 1.5
# On a raft each base is a foot this much wider than a line.
RAFT_FOOT_SPREAD = 1.15
# How far past the sloped edge and the wall the raft reaches.
RAFT_MARGIN = 0.2
RAFT_OFF = (None, "none", "off", False)


def _inset(poly: np.ndarray, d: float) -> np.ndarray:
    """A convex counter-clockwise polygon moved in by ``d``."""
    prev = np.roll(poly, 1, axis=0)
    nxt = np.roll(poly, -1, axis=0)
    e_in = poly - prev
    e_out = nxt - poly
    n_in = np.stack([e_in[:, 1], -e_in[:, 0]], 1)
    n_out = np.stack([e_out[:, 1], -e_out[:, 0]], 1)
    n_in /= np.linalg.norm(n_in, axis=1)[:, None]
    n_out /= np.linalg.norm(n_out, axis=1)[:, None]
    bis = n_in + n_out
    bis /= np.linalg.norm(bis, axis=1)[:, None]
    cos_half = np.clip(np.sum(bis * n_out, axis=1), 0.2, 1.0)
    return poly - (d / cos_half)[:, None] * bis


def _wall_piece(outer: np.ndarray, inner: np.ndarray, z0: float, z1: float):
    """A closed stretch of wall between two matching polylines."""
    m = len(outer)
    verts = np.concatenate([
        np.column_stack([outer, np.full(m, z0)]),
        np.column_stack([outer, np.full(m, z1)]),
        np.column_stack([inner, np.full(m, z0)]),
        np.column_stack([inner, np.full(m, z1)]),
    ])
    p0, p1, q0, q1 = 0, m, 2 * m, 3 * m
    quads = []
    for k in range(m - 1):
        quads += [
            (p0 + k, p0 + k + 1, p1 + k + 1, p1 + k),  # outside
            (p1 + k, p1 + k + 1, q1 + k + 1, q1 + k),  # top
            (q0 + k + 1, q0 + k, q1 + k, q1 + k + 1),  # inside
            (p0 + k + 1, p0 + k, q0 + k, q0 + k + 1),  # bottom
        ]
    e = m - 1
    quads += [(p0, p1, q1, q0), (p0 + e, q0 + e, q1 + e, p1 + e)]  # ends
    tris = [t for a, b, c, d in quads for t in ((a, b, c), (a, c, d))]
    return verts, np.array(tris)


def _raft_mesh(bases: list[tuple], thickness: float, wall: bool):
    """The raft under ``bases``: the hull around them, grown to clear the
    sloped edge and the wall."""
    t = np.arange(32) * (2 * math.pi / 32)
    ring = np.stack([np.cos(t), np.sin(t)], 1)
    margin = RAFT_MARGIN + max(thickness, RAFT_WALL_THICKNESS if wall else 0.0)
    pts = np.concatenate([c[None, :2] + (r + margin) * ring for c, r in bases])
    floor = min(c[2] for c, _ in bases)
    top = _hull(pts)
    n = len(top)
    if n < 3:
        return None
    bottom = _inset(top, min(thickness, 0.25 * float(np.min(np.ptp(top, axis=0)))))
    verts = np.concatenate([
        np.column_stack([bottom, np.full(n, floor)]),
        np.column_stack([top, np.full(n, floor + thickness)]),
    ])
    i = np.arange(n)
    j = (i + 1) % n
    tris = [np.stack([i, j, n + j], 1), np.stack([i, n + j, n + i], 1)]
    k = np.arange(1, n - 1)
    tris.append(np.stack([np.zeros_like(k), k + 1, k], 1))  # floor, facing down
    tris.append(np.stack([np.full_like(k, n), n + k, n + k + 1], 1))  # top, facing up
    parts = [(verts, np.concatenate(tris))]
    if wall:
        parts += _wall(top, floor + thickness)
    return parts


def _wall(outline: np.ndarray, z: float) -> list:
    inner = _inset(outline, RAFT_WALL_THICKNESS)
    outer_loop = np.vstack([outline, outline[:1]])
    inner_loop = np.vstack([inner, inner[:1]])
    lengths = np.linalg.norm(np.diff(outer_loop, axis=0), axis=1)
    at = np.concatenate([[0.0], np.cumsum(lengths)])
    total = float(at[-1])
    periods = max(1, round(total / RAFT_WALL_SPACING))
    period = total / periods
    gap = min(RAFT_WALL_GAP, 0.5 * period)

    def point(s):
        e = min(int(np.searchsorted(at, s, side="right")) - 1, len(lengths) - 1)
        f = (s - at[e]) / lengths[e] if lengths[e] > 0 else 0.0
        return (
            outer_loop[e] + f * (outer_loop[e + 1] - outer_loop[e]),
            inner_loop[e] + f * (inner_loop[e + 1] - inner_loop[e]),
        )

    pieces = []
    for p in range(periods):
        s0, s1 = p * period, (p + 1) * period - gap
        stops = [s0] + [float(a) for a in at if s0 < a < s1] + [s1]
        outer, inner_pts = zip(*(point(s) for s in stops))
        pieces.append(_wall_piece(np.array(outer), np.array(inner_pts), z, z + RAFT_WALL_HEIGHT))
    return pieces


def shapes(document: dict) -> Shapes:
    """The shapes of every piece in a supports document."""
    state = document.get("state")
    if not isinstance(state, dict):
        raise SupportsError("the supports document holds no supports")
    out = Shapes()

    def coll(name: str) -> list[dict]:
        v = state.get(name)
        if isinstance(v, dict):
            return [x for x in v.values() if isinstance(x, dict)]
        if isinstance(v, list):
            return [x for x in v if isinstance(x, dict)]
        return []

    knots = {k.get("id"): k for k in coll("knots")}
    roots = {r.get("id"): r for r in coll("roots")}

    # With one raft for every model ("oneRaft"), Breadslicer makes it of
    # lines and keeps it as a mesh of the scene, so the supports bring none of
    # their own; their bases stand on it all the same.
    options = (document.get("recipe") or {}).get("options") or {}
    mode = "lines" if options.get("oneRaft") else options.get("raft")
    on_raft = mode not in RAFT_OFF
    raft_height = RAFT_LINE_HEIGHT if mode == "lines" else _num(options, "raftThicknessMm")

    def knot_pos(knot_id):
        return _vec(knots.get(knot_id, {}).get("pos"))

    # Each base's top, where its trunk starts, and the trunk sizes.
    trunk_radius = {}
    for piece in coll("trunks") + coll("kickstands"):
        d = _num(piece, "baseDiameterMm") or _num(piece.get("profile"), "bodyDiameterMm")
        segments = piece.get("segments") or []
        if not d and segments:
            d = _num(segments[0], "diameter")
        trunk_radius[piece.get("rootId")] = d / 2

    tops = {}

    def root_top(root_id):
        if root_id not in tops:
            tops[root_id] = _root_top(root_id)
        return tops[root_id]

    def _root_top(root_id):
        r = roots.get(root_id)
        if r is None:
            return None
        tf = r.get("transform") or {}
        pos = _vec(tf.get("pos"))
        if pos is None:
            return None
        up = _unit(_rotate(tf.get("rot"), np.array([0.0, 0.0, 1.0])))
        if up is None:
            up = np.array([0.0, 0.0, 1.0])
        return pos + up * (_num(r, "diskHeight") + _num(r, "coneHeight"))

    bases = []
    for r in roots.values():
        tf = r.get("transform") or {}
        pos = _vec(tf.get("pos"))
        if pos is None:
            continue
        up = _unit(_rotate(tf.get("rot"), np.array([0.0, 0.0, 1.0])))
        if up is None:
            up = np.array([0.0, 0.0, 1.0])
        radius = _num(r, "diameter") / 2
        disk = _num(r, "diskHeight")
        cone = _num(r, "coneHeight")
        trunk = trunk_radius.get(r.get("id"), radius / 4)
        if on_raft:
            # A pad as tall as the raft and a 45° cone up to the trunk, which
            # starts inside the pad.
            foot = max(trunk, RAFT_LINE_WIDTH / 2 * RAFT_FOOT_SPREAD)
            pad = max(0.01, raft_height)
            rise = max(0.05, foot - trunk)
            out.cylinder(pos, pos + up * pad, foot)
            out.cylinder(pos + up * pad, pos + up * (pad + rise), foot, trunk)
            bases.append((pos, foot))
            continue
        out.cylinder(pos, pos + up * disk, radius)
        top = pos + up * disk
        if cone > 0:
            out.cylinder(top, top + up * cone, radius, trunk)
            top = top + up * cone
        join = _num(r, "joinConeMm")
        if join > 0:
            out.cylinder(top, top + up * join, min(trunk + join, radius), trunk)
        bases.append((pos, radius))

    def chain(piece, start=None, end=None):
        """The cylinders and balls of a piece's segments, and its joints by id."""
        joints = {}
        segments = [s for s in piece.get("segments") or [] if isinstance(s, dict)]
        tops, bottoms = [], []
        for s in segments:
            for key in ("topJoint", "bottomJoint"):
                j = s.get(key)
                if isinstance(j, dict) and _vec(j.get("pos")) is not None:
                    joints[j.get("id")] = (_vec(j["pos"]), _num(j, "diameter") / 2)
            tops.append(_vec((s.get("topJoint") or {}).get("pos")))
            bottoms.append(_vec((s.get("bottomJoint") or {}).get("pos")))
        for k, s in enumerate(segments):
            bottom = bottoms[k]
            if bottom is None:
                bottom = tops[k - 1] if k > 0 else start
            top = tops[k]
            if top is None:
                top = bottoms[k + 1] if k + 1 < len(segments) else end
            out.cylinder(bottom, top, _num(s, "diameter") / 2)
        for pos, radius in joints.values():
            out.ball(pos, radius)
        return joints, segments

    def tip(cone, joints, segments, anchor=None):
        """A tip: a thin disk on the model and a cone to the support."""
        if not isinstance(cone, dict):
            return
        pos = _vec(cone.get("pos"))
        if pos is None:
            return
        profile = cone.get("profile") if isinstance(cone.get("profile"), dict) else {}
        socket = joints.get(cone.get("socketJointId"))
        if socket is not None:
            anchor = socket[0]
        if anchor is None and joints:
            anchor = min((j[0] for j in joints.values()), key=lambda p: np.linalg.norm(p - pos))
        axis = _vec(cone.get("normal")) if cone.get("normal") else _vec(cone.get("coneAxis"))
        if anchor is not None:
            axis = _unit(anchor - pos)
        else:
            axis = _unit(axis) if axis is not None else None
            if axis is None:
                return
            anchor = pos + axis * (_num(profile, "lengthMm", 2.0) + _num(profile, "diskThicknessMm"))
        if axis is None:
            return
        contact = (_num(profile, "contactDiameterMm") or _num(cone, "contactDiameterMm")) / 2
        body = _num(profile, "bodyDiameterMm") / 2
        if not body:
            body = max((_num(s, "diameter") for s in segments), default=2 * contact) / 2
        start = pos - axis * _num(profile, "penetrationMm")
        if profile.get("type") == "disk":
            disk = _num(profile, "diskThicknessMm")
            out.cylinder(start, pos + axis * disk, contact)
            out.cylinder(pos + axis * disk, anchor, contact, body)
        else:
            out.ball(pos, contact)
            out.cylinder(start, anchor, contact, body)

    for piece in coll("trunks"):
        joints, segments = chain(piece, start=root_top(piece.get("rootId")))
        tip(piece.get("contactCone"), joints, segments)
    for piece in coll("branches"):
        joints, segments = chain(piece, start=knot_pos(piece.get("parentKnotId")))
        tip(piece.get("contactCone"), joints, segments)
    for piece in coll("leaves"):
        tip(piece.get("contactCone"), {}, [], anchor=knot_pos(piece.get("parentKnotId")))
    for piece in coll("sticks"):
        joints, segments = chain(piece)
        tip(piece.get("contactConeA"), joints, segments)
        tip(piece.get("contactConeB"), joints, segments)
    for piece in coll("twigs"):
        joints, segments = chain(piece)
        tip(piece.get("contactDiskA"), joints, segments)
        tip(piece.get("contactDiskB"), joints, segments)
    for piece in coll("kickstands"):
        chain(
            piece,
            start=root_top(piece.get("rootId")),
            end=knot_pos(piece.get("hostKnotId")),
        )
    for piece in coll("stumps"):
        base = _vec(piece.get("rootPos"))
        height = _num(piece, "rootHeight")
        top = base + np.array([0.0, 0.0, height]) if base is not None else None
        out.cylinder(base, top, _num(piece, "rootBaseDiameter") / 2, _num(piece, "rootTopDiameter") / 2)
        joint = piece.get("joint") if isinstance(piece.get("joint"), dict) else {}
        jpos = _vec(joint.get("pos"))
        out.ball(jpos, _num(joint, "diameter") / 2)
        out.cylinder(top, jpos, _num(piece, "rootTopDiameter") / 2)
        joints, segments = chain(piece, start=jpos if jpos is not None else top)
        tip(piece.get("contactCone"), joints, segments)
    for piece in coll("braces"):
        d = _num(piece.get("profile"), "diameter")
        out.cylinder(knot_pos(piece.get("startKnotId")), knot_pos(piece.get("endKnotId")), d / 2)
    for k in knots.values():
        out.ball(_vec(k.get("pos")), _num(k, "diameter") / 2)

    # A line raft of a set's own is drawn solid: the document does not keep
    # where its lines go.
    if on_raft and not options.get("oneRaft") and raft_height > 0 and bases:
        out.raft = (bases, raft_height, options.get("raftWall") is not False)
    return out


def build(document: dict, detail: str = "MEDIUM") -> tuple[np.ndarray, np.ndarray]:
    """The mesh of a supports document: vertices (n, 3) float32, in the
    document's space, and triangles (m, 3) uint32."""
    sides = DETAIL_SIDES.get(detail, DETAIL_SIDES["MEDIUM"])
    s = shapes(document)
    parts = []
    if s.cylinders:
        parts.append(_cylinder_mesh(s.cylinders, sides))
    if s.balls:
        parts.append(_ball_mesh(s.balls, sides))
    if s.raft is not None:
        parts += _raft_mesh(*s.raft) or []
    verts, tris, offset = [], [], 0
    for v, t in parts:
        verts.append(v)
        tris.append(t + offset)
        offset += len(v)
    if not verts:
        return np.zeros((0, 3), np.float32), np.zeros((0, 3), np.uint32)
    return (np.concatenate(verts).astype(np.float32), np.concatenate(tris).astype(np.uint32))


def read_document(generator: str, version: int, document: bytes) -> dict:
    """The JSON of a supports document this builder reads, or SupportsError."""
    if generator != GENERATOR:
        raise SupportsError(f'the supports were made by "{generator}", which this importer does not know')
    if version not in VERSIONS:
        raise SupportsError(f"the supports document is version {version}, newer than this importer reads")
    try:
        return json.loads(document.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise SupportsError(f"the supports document could not be read: {e}") from None
