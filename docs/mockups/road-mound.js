// Where the surface under a HIGHWAY comes from, as arithmetic alone.
//
// No THREE in this file on purpose: everything here is numbers, so the page
// draws it and a headless run can MEASURE it, and the claim the page makes
// ("mode 3 holds at every cell") is a thing a second person can check rather
// than a thing a picture asserts. It is the mockup's half of this project's
// own rule that the drawn thing and the collided thing are one set of
// numbers: `roadTop` below is read by the mesh, by the walker's feet and by
// the measurement, and there is no second copy of it anywhere.
//
// Directions are plain {x, y, z} unit vectors rather than a vector class,
// for the same reason.

import { fbm3 } from './common.js';

// ---------------------------------------------------------------- the body --
// Swept rather than chosen: at LUMPS 16 and three octaves a 220 m stretch
// swings 11.7 m with a worst ground grade of 36%, which is country a road
// must EMBANK across, because a road may only climb 10%. Six octaves came
// back at 140% over 42 turns, which is not hills, it is noise.
export const R = 1200, LUMPS = 16, RELIEF = 34, OCT = 3, SEED = 7;

// The patch's own frame on the body: a direction, and east and north across
// it. Written out rather than taken from `frameAt` so this file needs no
// vector class.
const norm = (v) => { const L = Math.hypot(v.x, v.y, v.z) || 1; return { x: v.x / L, y: v.y / L, z: v.z / L }; };
const cross = (a, b) => ({ x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x });
const dot = (a, b) => a.x * b.x + a.y * b.y + a.z * b.z;
export const UP = norm({ x: 0.04, y: 1, z: 0.02 });
export const EAST = norm(cross({ x: 0, y: 1, z: 0 }, UP));
export const NORTH = norm(cross(UP, EAST));

// Local metres on the patch to a direction on the body, and back. Through
// the SPHERE rather than over a plane: a 220 m stretch on a 1,200 m body is
// ten degrees of arc, and what a surface does over that arc is the question.
export const dirAt = (x, z) => norm({
  x: UP.x + (EAST.x * x + NORTH.x * z) / R,
  y: UP.y + (EAST.y * x + NORTH.y * z) / R,
  z: UP.z + (EAST.z * x + NORTH.z * z) / R,
});
export const localOf = (d) => { const u = norm(d); return { x: dot(u, EAST) * R, z: dot(u, NORTH) * R }; };

// The BARE relief at a direction, metres over the mean radius: the country
// as it was before anybody built a road. `field::Planet::surface`, no sites.
export const bare = (d) => (fbm3(d.x * LUMPS, d.y * LUMPS, d.z * LUMPS, SEED, OCT) * 2 - 1) * RELIEF * 0.5;
export const bareAt = (x, z) => bare(dirAt(x, z));

// ------------------------------------------------------------- the section --
// `road/ribbon.rs`'s own cross section and `road.rs`'s own corridor.
export const LANE = 2.75, HALF = LANE, SHOULDER = 0.7, LIFT = 0.15;
export const FLAT = HALF + SHOULDER;   // 3.45 m of surfaced road either side
export const CROSSFALL = 0.04;         // the shoulder's own fall, per metre
export const CORRIDOR = 16.0, SKIRT_IN = 5.0, SKIRT_OUT = 6.0;
export const BURIED = 0.15;            // how far the COPIED mound is sunk
export const GRADE = 0.10;             // `road::STEEPEST`, the steepest a road climbs
export const SPAN = 110, STATION = 5;  // the stretch, and its station spacing
export const TOE_BURY = 1.5;           // how deep the batter's toe is buried

const smoothstep = (a, b, t) => { const k = Math.min(1, Math.max(0, (t - a) / (b - a))); return k * k * (3 - 2 * k); };

// ----------------------------------------------------------- the alignment --
// Stations along the patch, bending so the road is not a straight bar.
export const line = [];
for (let x = -SPAN; x <= SPAN + 1e-9; x += STATION) {
  line.push({ x, z: 17 * Math.sin(x / 62) + 6 * Math.sin(x / 23) });
}
export const ground0 = line.map((p) => bareAt(p.x, p.z));

// The PROFILE a road is built at: `road::smooth`'s rule, which is that a road
// only ever RISES. Clamping the grade BOTH ways is real engineering and means
// CUTTING, and a cut is the one thing this terrain cannot draw at a coarse
// cell. Taking the MAX both ways leaves the least profile above the ground
// that the grade allows, so the road starts a climb earlier and stands on an
// embankment rather than being pulled down into the hill. That is what makes
// a MOUND sufficient: the profile is never under the ground, so the road
// never needs to cut and a mound is the only shape it ever wants.
export function profileFor(minFill) {
  const p = ground0.map((g) => g + minFill);
  for (let i = 1; i < p.length; i++) p[i] = Math.max(p[i], p[i - 1] - GRADE * STATION);
  for (let i = p.length - 2; i >= 0; i--) p[i] = Math.max(p[i], p[i + 1] - GRADE * STATION);
  return p;
}

// Where a point falls on the road: how far ALONG and ACROSS, and the profile
// there. The nearest of a few dozen stations, the mockup's `Site::nearest`.
export function onRoad(profile, x, z) {
  let best = null;
  for (let k = 0; k + 1 < line.length; k++) {
    const a = line[k], b = line[k + 1];
    const dx = b.x - a.x, dz = b.z - a.z, len2 = dx * dx + dz * dz;
    const t = Math.max(0, Math.min(1, ((x - a.x) * dx + (z - a.z) * dz) / len2));
    const px = a.x + dx * t, pz = a.z + dz * t;
    const d = Math.hypot(x - px, z - pz);
    if (!best || d < best.across) best = { across: d, k, t, H: profile[k] + (profile[k + 1] - profile[k]) * t };
  }
  return best;
}

// The unit vector ACROSS the road at a station, in local metres.
export function normalAt(k) {
  const a = line[Math.max(0, k - 1)], b = line[Math.min(line.length - 1, k + 1)];
  const dx = b.x - a.x, dz = b.z - a.z, L = Math.hypot(dx, dz) || 1;
  return { x: dz / L, z: -dx / L };
}

// THE ROAD'S OWN SURFACE at an across distance: tarmac, the shoulder's
// crossfall, then the BATTER, a straight line off the shoulder's edge.
//
// This is the whole of what the owner asked for. Every term is the ROAD's:
// its profile, its width, its batter. Nothing here samples the world, so
// nothing here can be lost when the mesher draws the world at a cell too
// coarse to have seen a road in it.
export function roadTop(across, H, batterGrade) {
  if (across <= HALF) return H + LIFT;
  if (across <= FLAT) return H + LIFT - (across - HALF) * CROSSFALL;
  return H + LIFT - SHOULDER * CROSSFALL - (across - FLAT) * batterGrade;
}

// And the TOE, which is the ONE place a mound reads the world.
//
// A batter is a straight line, so left alone it runs on for ever: it would
// hang in the air where a side hill falls away and come back OUT of the
// ground as a brown patch wherever the ground dropped past the end of it. So
// the toe is `max(the road's own batter, the ground less TOE_BURY)`: the
// batter is the road's own line for the whole of the part anybody can see,
// and the moment it is under the natural ground it follows it down, buried,
// which is what the toe of a real embankment is.
//
// What this is NOT is the mound it replaces, where EVERY vertex, the verge
// and the whole batter included, was `Planet::surface` sunk 15 cm. That was
// a copy of the world drawn as road: the levelling a coarse cell missed was
// the levelling the mound was made OF, so it was swallowed along with it.
export function moundTop(across, H, x, z, batterGrade) {
  return Math.max(roadTop(across, H, batterGrade), bareAt(x, z) - TOE_BURY);
}

// How far out the batter must reach to be under the ground everywhere on the
// stretch: the deepest fill over the batter's own grade. Worked out ONCE, so
// every station has the same band count and the quads between two of them
// close.
export function reachFor(profile, batterGrade) {
  let deep = 0;
  for (let k = 0; k < line.length; k++) deep = Math.max(deep, profile[k] - ground0[k]);
  return { reach: FLAT + (deep + TOE_BURY + 1) / batterGrade, deep };
}

// The LEVELLED surface: the field with a corridor cut into it, which is what
// the game does today and what modes 1 and 2 are drawn against.
export function levelled(profile, x, z) {
  const on = onRoad(profile, x, z);
  const w = 1 - smoothstep(CORRIDOR - SKIRT_IN, CORRIDOR + SKIRT_OUT, on.across);
  const g = bareAt(x, z);
  return w <= 0 ? g : g + (on.H - g) * w;
}

// -------------------------------------------------------------- the lattice --
// What a mesher with a given CELL can see: the surface at its own corners,
// joined by chords. The corners are snapped to the cell, so changing it
// really changes where the world is known rather than merely how many
// triangles say so.
export const WIDE = 95;
export function buildLattice(profile, cell, levels) {
  const x0 = Math.floor(-SPAN * 1.05 / cell) * cell, z0 = Math.floor(-WIDE / cell) * cell;
  const nx = Math.ceil((SPAN * 2.1) / cell) + 2, nz = Math.ceil((WIDE * 2) / cell) + 2;
  const h = new Float64Array(nx * nz);
  for (let j = 0; j < nz; j++) for (let i = 0; i < nx; i++) {
    const x = x0 + i * cell, z = z0 + j * cell;
    h[j * nx + i] = levels ? levelled(profile, x, z) : bareAt(x, z);
  }
  return { cell, nx, nz, x0, z0, h };
}

// The height the terrain is DRAWN at between its corners: the chord, which
// is what an eye sees and what the measurement has to compare against.
export function drawnTerrain(lat, x, z) {
  const { cell, nx, nz, x0, z0, h } = lat;
  const fx = (x - x0) / cell, fz = (z - z0) / cell;
  const i = Math.max(0, Math.min(nx - 2, Math.floor(fx))), j = Math.max(0, Math.min(nz - 2, Math.floor(fz)));
  const tx = Math.max(0, Math.min(1, fx - i)), tz = Math.max(0, Math.min(1, fz - j));
  const a = h[j * nx + i], b = h[j * nx + i + 1], c = h[(j + 1) * nx + i], d = h[(j + 1) * nx + i + 1];
  const top = a + (b - a) * tx, bot = c + (d - c) * tx;
  return top + (bot - top) * tz;
}

// ---------------------------------------------------------- the collision --
// THE MOUND IS WHAT YOUR FEET ARE ON, off the same `moundTop` the mesh is
// built from. Mode 3 alone has a surface of its own to stand on: mode 1 has
// no mound, and mode 2's mound IS the levelled field, so it adds nothing a
// walker could not already stand on.
export const MODE = { LEVEL: 0, COPY: 1, ROAD: 2 };
export function surfaceUnder(profile, x, z, mode, batterGrade) {
  const terrain = mode === MODE.ROAD ? bareAt(x, z) : levelled(profile, x, z);
  if (mode !== MODE.ROAD) return { y: terrain, road: false, across: onRoad(profile, x, z).across };
  const on = onRoad(profile, x, z);
  const road = moundTop(on.across, on.H, x, z, batterGrade);
  return road > terrain
    ? { y: road, road: true, across: on.across }
    : { y: terrain, road: false, across: on.across };
}

// ------------------------------------------------------------ the measure --
// The ONE number that decides this: how far the terrain the mesher DRAWS
// stands over the tarmac, along the whole stretch. Positive is the hill
// winning, which is the road being swallowed.
export function overTarmac(profile, lat) {
  let worst = -1e9; const all = [];
  for (let k = 0; k + 1 < line.length; k++) for (let t = 0; t < 1; t += 0.1) {
    const a = line[k], b = line[k + 1];
    const x = a.x + (b.x - a.x) * t, z = a.z + (b.z - a.z) * t;
    const H = profile[k] + (profile[k + 1] - profile[k]) * t + LIFT;
    const over = drawnTerrain(lat, x, z) - H;
    worst = Math.max(worst, over); all.push(over);
  }
  all.sort((p, q) => p - q);
  return { worst, median: all[all.length >> 1] };
}
