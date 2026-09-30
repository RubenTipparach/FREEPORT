#!/usr/bin/env python3
"""Faces that fight for one plane in the baked building library: what the
owner saw as z-fighting, measured on the meshes rather than looked for in
a picture.

    python tools/coplanar.py                      # every variant, every LOD
    python tools/coplanar.py assets/models/buildings

Two triangles facing the same way within a millimetre of one plane, where
they overlap and a camera can see it, fight for every pixel of the overlap.
Not seen is a face pressed down on the ground, or one with another of the
building's own solids standing on it (a wall's top under a slab). It is
`model/tests/coplanar.rs`'s rule for the parametric buildings and the
streets, arriving at the bakes: the old library carried 63 to 98 such pairs
in every variant but the towers, most of them a floor slab whose sides lay
in the walls' planes, and a bake that brings one back fails here.
Exits 1 when any variant has one.
"""
import json
import sys
from pathlib import Path

import numpy as np


def cross2(a, b):
    return a[0] * b[1] - a[1] * b[0]


def overlap(a, b):
    """The area two triangles in one plane both cover, and its middle."""
    def ccw(t):
        return t if cross2(t[1] - t[0], t[2] - t[0]) > 0 else t[::-1]

    poly, clip = list(ccw(a)), list(ccw(b))
    for k in range(3):
        e0, e1 = clip[k], clip[(k + 1) % 3]
        nxt = []
        for n in range(len(poly)):
            p0, p1 = poly[n], poly[(n + 1) % len(poly)]
            d0, d1 = cross2(e1 - e0, p0 - e0), cross2(e1 - e0, p1 - e0)
            if d0 > 1e-9:
                nxt.append(p0)
            if (d0 > 1e-9) != (d1 > 1e-9):
                nxt.append(p0 + (p1 - p0) * (d0 / (d0 - d1)))
        poly = nxt
        if len(poly) < 3:
            return None
    area = abs(sum(cross2(poly[n], poly[(n + 1) % len(poly)]) for n in range(len(poly)))) * 0.5
    return area, sum(poly) / len(poly)


def buried(solids, q):
    """Whether a point is inside one of the building's own solids."""
    for s in solids:
        c, h, yaw = np.array(s["centre"]), np.array(s["half"]), s.get("yaw", 0.0)
        cs, sn = np.cos(yaw), np.sin(yaw)
        axes = (np.array([cs, sn, 0.0]), np.array([-sn, cs, 0.0]), np.array([0.0, 0.0, 1.0]))
        d = q - c
        if all(abs(d @ axes[k]) < h[k] - 1e-4 for k in range(3)):
            return True
    return False


def fights(lod, solids):
    """Every visible coplanar pair of one LOD's triangles."""
    pos = np.array(lod["positions"], dtype=float)
    tris = pos[np.array(lod["indices"]).reshape(-1, 3)]
    normals = np.cross(tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0])
    lens = np.linalg.norm(normals, axis=1)
    live = lens > 1e-12
    normals[live] /= lens[live][:, None]
    planes = {}
    for i in np.nonzero(live)[0]:
        n = normals[i]
        planes.setdefault((tuple(np.round(n, 3)), round(float(n @ tris[i][0]), 2)), []).append(i)
    found = []
    for (nk, _), members in planes.items():
        if len(members) < 2:
            continue
        n = np.array(nk)
        u = np.cross(n, [1.0, 0, 0]) if abs(n[0]) < 0.9 else np.cross(n, [0, 1.0, 0])
        u /= np.linalg.norm(u)
        v = np.cross(n, u)
        flat = {i: np.array([[p @ u, p @ v] for p in tris[i]]) for i in members}
        for a in range(len(members)):
            for b in range(a + 1, len(members)):
                i, j = members[a], members[b]
                if abs(n @ (tris[j][0] - tris[i][0])) > 1e-3:
                    continue
                got = overlap(flat[i], flat[j])
                if not got or got[0] < 1e-4:
                    continue
                at = u * got[1][0] + v * got[1][1] + n * (n @ tris[i][0])
                if (n[2] < -0.99 and at[2] < 0.01) or buried(solids, at + n * 0.005):
                    continue
                found.append((int(i), int(j), round(float(got[0]), 3), np.round(at, 2).tolist()))
    return found


def main():
    root = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "assets/models/buildings")
    bad = 0
    for f in sorted(root.glob("*.json")):
        if f.name == "manifest.json":
            continue
        d = json.loads(f.read_text())
        for k, lod in enumerate(d["lods"]):
            found = fights(lod, d["solids"])
            if found:
                bad += 1
                print(f"{f.name} LOD {k}: {len(found)} visible coplanar pairs, e.g. {found[:2]}")
    print("coplanar: ok" if not bad else f"coplanar: {bad} variant LODs FAIL")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
