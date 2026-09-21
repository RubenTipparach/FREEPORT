#!/usr/bin/env python3
"""Fetch REAL city plans from OpenStreetMap for the city blocks mockup.

The mockup used to draw an invented uniform grid under each city's NAME,
which is a picture of a number rather than a picture of a city: no alleys,
no diagonals, no river, no park, no irregular block and none of the
variety that is most of what a real plan looks like. This pulls the actual
geometry, streets and building footprints both.

WHAT IS MEASURED AND WHAT IS ESTIMATED, which is the whole point of doing
this rather than quoting a block size:

  * COVERAGE is measured. It is the share of the ground that carries a
    building footprint, off OSM's own polygons, with nothing guessed. It
    is the number the mockup leads with, because it is the one FREEPORT
    can be held against: one 10 m building on a 19 m pitch is 27.7% and a
    real downtown is far denser.
  * The STREET share is an ESTIMATE and is labelled one. OSM stores a
    street as a CENTRELINE, so how much ground it occupies has to come
    from somewhere: `width` where the map has it, else `lanes` times a
    lane, else a table by class. The tool reports how many ways got their
    width from a tag rather than from the table, so a reader can see how
    much of the number is measured.

The SOURCE is OpenStreetMap, and this script plus the box it records is
what makes the extract regenerable. A byte for byte `--check` would be
meaningless, because the map changes under us; what the file records is
the query, the box and the date, which is what a reader needs to get the
same answer or to see why theirs differs.

Map data (c) OpenStreetMap contributors, ODbL. The page it feeds says so
and so does the file this writes.

    python3 tools/fetch_city_grids.py            # write docs/mockups/city-grids.js
    python3 tools/fetch_city_grids.py --dry      # fetch and report, write nothing
    python3 tools/fetch_city_grids.py --only chicago
"""

import argparse
import datetime as dt
import json
import math
import pathlib
import sys
import time
import urllib.parse
import urllib.request

import numpy as np

# OpenStreetMap's OWN api is the source, and Overpass is the fallback.
# Measured on the same Portland box: the api answered a quarter of the
# ground in 1.6 SECONDS where Overpass took minutes and then handed back
# a 504 from every mirror in turn. The api also gives the whole bbox in
# ONE request rather than one query per kind of feature, so a city is one
# call instead of five. Its own limit is 50,000 nodes; a 780 m square of
# downtown is about eleven thousand, and `quarters` is what a denser box
# would be split into.
#
# Overpass's own main instance refuses this container outright with a 406
# and private.coffee answers when it is not overloaded, which is written
# down here so nobody spends an afternoon finding it twice.
OSM_API = "https://api.openstreetmap.org/api/0.6/map.json"

MIRRORS = [
    "https://overpass.private.coffee/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass-api.de/api/interpreter",
]
TRIES = 3
WAIT = 90.0          # seconds to let one mirror think before trying the next
GAP = 3.0            # seconds between requests: a public instance asks to be
                     # asked politely, and hammering one is what turns a
                     # working mirror into a wall of 504s

# Overpass asks every client to identify itself, and an anonymous one is
# the first to be throttled. This says what the tool is and where it lives.
AGENT = ("freeport-city-grids/1.0 (a game's mockup; "
         "github.com/RubenTipparach/FREEPORT)")

# A square of real ground about each city's own downtown. It is the frame
# the mockup draws, so the extract is exactly what is shown and never a
# kilometre of data clipped to a third of it.
SPAN = 760.0

CITIES = [
    dict(key="portland", name="Portland, OR", lat=45.5202, lon=-122.6790,
         note="200 ft block, 60 ft right of way"),
    dict(key="barcelona", name="Barcelona Eixample", lat=41.3910, lon=2.1650,
         note="Cerda's octagonal block, chamfered at every corner"),
    dict(key="chicago", name="Chicago Loop", lat=41.8820, lon=-87.6300,
         note="330 x 660 ft, an alley down the middle"),
    dict(key="manhattan", name="Manhattan", lat=40.7530, lon=-73.9830,
         note="street 60 ft, avenue 100 ft, Broadway cutting across"),
    dict(key="savannah", name="Savannah, GA", lat=32.0770, lon=-81.0920,
         note="Oglethorpe's wards, a square every four blocks"),
    dict(key="siena", name="Siena", lat=43.3180, lon=11.3310,
         note="no grid at all: a hill town that grew"),
]

# What counts as a STREET. `service` is kept only where it is an alley,
# because that is the thing a Chicago block is built round; a driveway or
# a car park aisle is not a street and would flatter every share here.
ROADS = ("motorway|trunk|primary|secondary|tertiary|residential|"
         "unclassified|living_street|pedestrian|service")
ROAD_SET = set(ROADS.split("|")) | {r + "_link" for r in
                                    ("motorway", "trunk", "primary",
                                     "secondary", "tertiary")}

# The RIGHT OF WAY a class occupies where the map does not say, in metres:
# carriageway plus its pavements, because that is what the page's own
# street share compares against a block. ESTIMATES, and the tool says how
# often it had to fall back to them.
ROW = {
    "motorway": 24.0, "trunk": 22.0, "primary": 20.0, "secondary": 18.0,
    "tertiary": 15.0, "residential": 13.0, "unclassified": 13.0,
    "living_street": 10.0, "pedestrian": 10.0, "alley": 5.0,
}
LINK, LANE, WALKS = 9.0, 3.1, 4.0


def box_of(city):
    """The bounding box, as Overpass wants it and as the file records it."""
    half = SPAN / 2.0
    dlat = half / 111_320.0
    dlon = half / (111_320.0 * math.cos(math.radians(city["lat"])))
    return (f"{city['lat'] - dlat:.6f},{city['lon'] - dlon:.6f},"
            f"{city['lat'] + dlat:.6f},{city['lon'] + dlon:.6f}")


def api(box):
    """The whole box off OpenStreetMap's own api, as ways with geometry.

    The api hands back nodes and ways separately where Overpass's
    `out geom` does the join for us, so the node table is built once and
    every way reads its own points off it. A way with a node outside the
    box is dropped rather than drawn with a gap in it.
    """
    s, w, n, e = box.split(",")
    url = f"{OSM_API}?bbox={w},{s},{e},{n}"      # the api wants it the other way round
    req = urllib.request.Request(url, headers={"User-Agent": AGENT})
    data = None
    for attempt in range(TRIES):
        try:
            time.sleep(GAP)
            with urllib.request.urlopen(req, timeout=WAIT) as r:
                data = json.loads(r.read().decode("utf-8"))
            break
        except Exception as exc:                  # noqa: BLE001
            print(f"      osm api: {exc}", file=sys.stderr)
            if attempt + 1 >= TRIES:
                return None
            time.sleep(8 * (attempt + 1))
    at = {el["id"]: el for el in data["elements"] if el["type"] == "node"}
    out = []
    for el in data["elements"]:
        if el["type"] != "way":
            continue
        refs = el.get("nodes", [])
        pts = [at[i] for i in refs if i in at]
        if len(pts) != len(refs) or len(pts) < 2:
            continue                              # a way that leaves the box
        out.append(dict(id=el["id"], tags=el.get("tags") or {},
                        geometry=[{"lat": p["lat"], "lon": p["lon"]} for p in pts]))
    return {"elements": out}


def keep(tags):
    """Whether this way is a STREET rather than a path or a driveway.

    Overpass filtered this in the query; the api hands back everything in
    the box, so the filter lives here. `service` is kept only where it is
    an ALLEY, because that is the thing a Chicago block is built round
    and a driveway is not a street.
    """
    hw = tags.get("highway", "")
    if hw not in ROAD_SET:
        return False
    return not (hw == "service" and tags.get("service") != "alley")


def fetch(ql):
    """Ask each mirror in turn, with a backoff, for the first answer that parses."""
    last = "nothing tried"
    for attempt in range(TRIES):
        for url in MIRRORS:
            full = url + "?" + urllib.parse.urlencode({"data": ql})
            req = urllib.request.Request(full, headers={"User-Agent": AGENT})
            try:
                time.sleep(GAP)
                with urllib.request.urlopen(req, timeout=WAIT) as r:
                    return json.loads(r.read().decode("utf-8"))
            except Exception as exc:                     # noqa: BLE001
                last = f"{url.split('/')[2]}: {exc}"
                print(f"      {last}", file=sys.stderr)
        if attempt + 1 < TRIES:
            time.sleep(8 * (attempt + 1))
    print(f"    GAVE UP: {last}", file=sys.stderr)
    return None


def metres(city):
    """Equirectangular metres about the box's own centre, which is exact
    enough over a few hundred: the error is second order in the box's own
    angular size and is well under a centimetre here. z grows SOUTH, so
    the projection and the plan agree about which way is up."""
    mlat = 111_320.0
    mlon = 111_320.0 * math.cos(math.radians(city["lat"]))
    return lambda p: [round((p["lon"] - city["lon"]) * mlon, 1),
                      round(-(p["lat"] - city["lat"]) * mlat, 1)]


def row_of(tags):
    """A way's right of way in metres, and whether that was read or guessed."""
    hw = tags.get("highway", "")
    if hw == "service":
        return ROW["alley"], False
    try:
        if "width" in tags:
            return float(str(tags["width"]).split()[0]) + WALKS, True
    except ValueError:
        pass
    try:
        if "lanes" in tags:
            return int(str(tags["lanes"]).split(";")[0]) * LANE + WALKS, True
    except ValueError:
        pass
    if hw.endswith("_link"):
        return LINK, False
    return ROW.get(hw, 13.0), False


def thin(pts, tol=0.35):
    """Drop a point that is within `tol` of the line its neighbours make,
    which is most of a building's points and none of its corners."""
    if len(pts) < 3:
        return pts
    out = [pts[0]]
    for a, b, c in zip(pts, pts[1:], pts[2:]):
        ax, az = c[0] - a[0], c[1] - a[1]
        n = math.hypot(ax, az)
        off = abs((b[0] - a[0]) * az - (b[1] - a[1]) * ax) / n if n else 0.0
        if off > tol:
            out.append(b)
    out.append(pts[-1])
    return out


def rasterise(span, cell=0.5):
    """An empty mask of the box and the transform onto it."""
    n = int(span / cell)
    return np.zeros((n, n), dtype=bool), n, cell


def stamp_ways(mask, n, cell, span, ways):
    """Walk every centreline and set the disc of its own width, which is
    what gives a junction its rounded corner for free."""
    half = span / 2.0
    ys, xs = np.mgrid[0:n, 0:n]
    for w in ways:
        r = w["row"] / 2.0
        ri = max(1, int(r / cell))
        for a, b in zip(w["pts"], w["pts"][1:]):
            steps = max(1, int(math.hypot(b[0] - a[0], b[1] - a[1]) / (cell * 2)))
            for s in range(steps + 1):
                t = s / steps
                cx = int((a[0] + (b[0] - a[0]) * t + half) / cell)
                cz = int((a[1] + (b[1] - a[1]) * t + half) / cell)
                i0, i1 = max(0, cx - ri), min(n, cx + ri + 1)
                j0, j1 = max(0, cz - ri), min(n, cz + ri + 1)
                if i0 >= i1 or j0 >= j1:
                    continue
                sub = (xs[j0:j1, i0:i1] - cx) ** 2 + (ys[j0:j1, i0:i1] - cz) ** 2
                mask[j0:j1, i0:i1] |= sub <= ri * ri


def stamp_polys(mask, n, cell, span, polys):
    """Even odd fill of every footprint, by scanline, which is exact for
    the closed rings OSM stores a building as."""
    half = span / 2.0
    for ring in polys:
        p = [((x + half) / cell, (z + half) / cell) for x, z in ring]
        lo = max(0, int(min(q[1] for q in p)))
        hi = min(n - 1, int(max(q[1] for q in p)))
        for j in range(lo, hi + 1):
            y = j + 0.5
            xs = []
            for (x0, y0), (x1, y1) in zip(p, p[1:] + p[:1]):
                if (y0 <= y) != (y1 <= y):
                    xs.append(x0 + (y - y0) * (x1 - x0) / (y1 - y0))
            xs.sort()
            for a, b in zip(xs[0::2], xs[1::2]):
                i0, i1 = max(0, int(math.ceil(a - 0.5))), min(n, int(b + 0.5))
                if i0 < i1:
                    mask[j, i0:i1] = True


def quarters(box):
    """The box cut in four, so a dense city is four small requests.

    Midtown Manhattan in one request is a 504 on every mirror: the box is
    dense enough that the query outruns the server's own timeout. Four
    quarters of the same box is four small requests over the same ground,
    which keeps every city the same 760 m square rather than shrinking the
    one that is hardest to get.
    """
    s, w, n, e = (float(v) for v in box.split(","))
    ms, me = (s + n) / 2, (w + e) / 2
    return [f"{a},{b},{c},{d}" for a, c in ((s, ms), (ms, n))
            for b, d in ((w, me), (me, e))]


def one(city):
    """One city: the query, the extract and the two shares."""
    box = box_of(city)
    print(f"{city['name']:<22} {box}")
    to_m = metres(city)

    got = api(box)
    if got is None:                               # fall back to Overpass
        got = fetch(f'[out:json][timeout:80];way({box});out geom;')
    if got is None:
        print("    nothing came back, so this city is skipped rather than half drawn",
              file=sys.stderr)
        return None

    ways, tagged = [], 0
    for el in got["elements"]:
        tags, geom = el.get("tags") or {}, el.get("geometry")
        if not geom or not keep(tags):
            continue
        pts = [to_m(p) for p in geom]
        if len(pts) < 2:
            continue
        row, read = row_of(tags)
        tagged += bool(read)
        ways.append(dict(hw=tags["highway"], row=round(row, 1), pts=thin(pts, 0.6)))

    builds = []
    for el in got["elements"]:
        geom = el.get("geometry")
        if "building" not in (el.get("tags") or {}) or not geom or len(geom) < 4:
            continue
        ring = thin([to_m(p) for p in geom])
        if len(ring) >= 4:
            builds.append(ring)

    road_mask, n, cell = rasterise(SPAN)
    stamp_ways(road_mask, n, cell, SPAN, ways)
    build_mask, _, _ = rasterise(SPAN)
    stamp_polys(build_mask, n, cell, SPAN, builds)
    street = float(road_mask.mean())
    cover = float(build_mask.mean())

    pts = sum(len(w["pts"]) for w in ways) + sum(len(b) for b in builds)
    share = f"{tagged}/{len(ways)}" if ways else "0/0"
    print(f"    {len(ways):4d} ways, {len(builds):5d} buildings, {pts:6d} points")
    print(f"    coverage {cover * 100:5.1f}% MEASURED   "
          f"street {street * 100:5.1f}% estimated ({share} ways carry a width)")
    return dict(name=city["name"], note=city["note"], span=SPAN, box=box,
                lat=city["lat"], lon=city["lon"],
                cover=round(cover, 4), street=round(street, 4),
                tagged=tagged, ways=ways, builds=builds)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dry", action="store_true", help="fetch and report, write nothing")
    ap.add_argument("--only", help="one city's key")
    ap.add_argument("--out", default="docs/mockups/city-grids.js")
    args = ap.parse_args()

    want = [c for c in CITIES if not args.only or c["key"] == args.only]
    path = pathlib.Path(args.out)
    out = read_back(path)
    for city in want:
        got = one(city)
        if got is None:
            continue
        out[city["key"]] = got
        if not args.dry:
            write(path, out)                 # after EVERY city, so a later
                                             # failure never costs an earlier
                                             # success a second slow fetch
    if args.dry:
        return
    print(f"\n{len(out)} cities in {path} ({path.stat().st_size / 1024:.0f} KB)")


def read_back(path):
    """Whatever is already extracted, so a re-run fetches only what is missing."""
    if not path.exists():
        return {}
    old = path.read_text(encoding="utf-8")
    head = old.index("window.CITY_GRIDS = ") + len("window.CITY_GRIDS = ")
    return json.loads(old[head:old.rindex(";")])


def write(path, out):
    path.write_text(
        "// REAL city plans, extracted from OpenStreetMap by\n"
        "// tools/fetch_city_grids.py. Map data (c) OpenStreetMap\n"
        "// contributors, ODbL. Each city is a %.0f m square of ground about\n"
        "// the point named, projected to metres with z growing south.\n"
        "// `cover` is MEASURED off the building footprints; `street` is an\n"
        "// ESTIMATE, because OSM stores a centreline and not a right of\n"
        "// way, and `tagged` says how many ways carried a real width.\n"
        "// Fetched %s.\n"
        "window.CITY_GRIDS = %s;\n"
        % (SPAN, dt.date.today().isoformat(),
           json.dumps(out, separators=(",", ":"))), encoding="utf-8")


if __name__ == "__main__":
    main()
