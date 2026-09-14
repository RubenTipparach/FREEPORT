// The brush kit: how a building is written into a density field. A recipe
// (assets/buildings/*.json, bundled into buildings.js) is a list of brushes
// in the building's own frame, x east, y north, z up, metres, origin at the
// middle of the footprint on the ground. Each brush is a signed distance
// (negative inside) that is ADDED to the field (union, or a smooth union
// with `blend`) or CUT from it (subtraction), in list order, so a door cut
// after a wall goes through the wall and a flight added after a room stands
// in the room. Nothing here knows about a planet: the page hands in local
// coordinates and gets back a density and a material, which is what lets
// the same list be evaluated by the core one day.
//
// The material rule: a sample's material is the DEEPEST solid at the point,
// the one whose surface is farthest away, because the mesher asks a hand
// inside a triangle's middle, and a thin thing sitting on a thick one (a
// street on the ground) has to win that sample or it draws as its host. So
// a thin thing is SUNK into what it stands on by at least a cell, and a
// skin is thicker than a cell or is the whole of the solid. The rule is in
// the README beside the recipes.

export const MAT = { terrain: 0, concrete: 1, plate: 2, glass: 3, lamp: 4, lit: 5, street: 6 };
export const MAT_NAMES = Object.keys(MAT);

const RAD = Math.PI / 180;
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

// Signed distance to a box of half sizes h, centred on the origin.
export function sdBox(x, y, z, hx, hy, hz) {
  const qx = Math.abs(x) - hx, qy = Math.abs(y) - hy, qz = Math.abs(z) - hz;
  const ox = Math.max(qx, 0), oy = Math.max(qy, 0), oz = Math.max(qz, 0);
  return Math.sqrt(ox * ox + oy * oy + oz * oz) + Math.min(Math.max(qx, Math.max(qy, qz)), 0);
}
// A capped cylinder: `rad` is the radial distance from the axis, `along` the
// coordinate on it.
function sdCyl(rad, along, r, h) {
  const dx = rad - r, dy = Math.abs(along) - h;
  const ox = Math.max(dx, 0), oy = Math.max(dy, 0);
  return Math.min(Math.max(dx, dy), 0) + Math.sqrt(ox * ox + oy * oy);
}

// A flight of steps: `steps` boxes rising along +y from the low end at
// -hy, each a tread deep and a riser higher than the last. Only the step
// under the point and its two neighbours are measured, which is exact near
// the flight and a fair bound away from it.
function sdStairs(x, y, z, hx, hy, hz, steps) {
  const a = y + hy, up = z + hz;
  const tread = 2 * hy / steps, riser = 2 * hz / steps;
  const j = clamp(Math.floor(a / tread), 0, steps - 1);
  let d = Infinity;
  for (let jj = Math.max(0, j - 1); jj <= Math.min(steps - 1, j + 1); jj++) {
    const top = (jj + 1) * riser;
    d = Math.min(d, sdBox(a - (jj + 0.5) * tread, x, up - top / 2, tread / 2, hx, top / 2));
  }
  return d;
}

// Smooth maximum of two densities: the union with a fillet of radius k in
// the crease, which is how a plinth is poured into the ground.
export function smax(a, b, k) {
  const h = clamp(0.5 + 0.5 * (b - a) / k, 0, 1);
  return a + (b - a) * h + k * h * (1 - h);
}

// The signed distance of one compiled brush at a point in the building
// frame: into the brush's own frame (its centre, then the inverse of its
// turn about up, its tilt about north and its pitch about east), the shape,
// then the clip, which is a slab in the BUILDING's up so a vault can be a
// cylinder from the wall line up and nothing below it.
export function sdBrush(b, e, n, u) {
  let qe = e - b.c[0], qn = n - b.c[1], qu = u - b.c[2];
  if (b.rot) { const c = b.rot[0], s = b.rot[1]; const te = c * qe + s * qn, tn = -s * qe + c * qn; qe = te; qn = tn; }
  if (b.tilt) { const c = b.tilt[0], s = b.tilt[1]; const te = c * qe + s * qu, tu = -s * qe + c * qu; qe = te; qu = tu; }
  if (b.pitch) { const c = b.pitch[0], s = b.pitch[1]; const tn = c * qn + s * qu, tu = -s * qn + c * qu; qn = tn; qu = tu; }
  const h = b.h;
  let d;
  switch (b.shape) {
    case 'box': d = sdBox(qe, qn, qu, h[0], h[1], h[2]); break;
    case 'sphere': d = Math.sqrt(qe * qe + qn * qn + qu * qu) - h[0]; break;
    case 'cyl':
      if (b.axis === 'n') d = sdCyl(Math.hypot(qe, qu), qn, Math.min(h[0], h[2]), h[1]);
      else if (b.axis === 'e') d = sdCyl(Math.hypot(qn, qu), qe, Math.min(h[1], h[2]), h[0]);
      else d = sdCyl(Math.hypot(qe, qn), qu, Math.min(h[0], h[1]), h[2]);
      break;
    case 'stairs': d = sdStairs(qe, b.dir === 's' ? -qn : qn, qu, h[0], h[1], h[2], b.steps); break;
    default: d = Infinity;
  }
  if (b.clip) d = Math.max(d, b.clip[0] - u, u - b.clip[1]);
  return d;
}

// Compose a building's brushes into a field sample at (e, n, u). `out` comes
// in carrying the ground's density and material and leaves carrying the
// building's: an add that is deeper than what is there wins the point and
// its material, a cut that is nearer than what is there takes it, and a cut
// that is a room names the room. `owner` is what the page wants back for a
// point this building's material won, and `roomBase` makes its room ids
// unique across the town.
export function evalBrushes(brushes, e, n, u, out, owner, roomBase) {
  for (const b of brushes) {
    const bb = b.bb;
    if (e < bb[0] || e > bb[1] || n < bb[2] || n > bb[3] || u < bb[4] || u > bb[5]) continue;
    const sd = sdBrush(b, e, n, u);
    if (b.op === 'add') {
      const v = -sd;
      // A curved brush, or a blended one, is shaded smooth; a box is shaded
      // flat, because its edges are edges.
      if (b.blend > 0) {
        const nd = smax(out.d, v, b.blend);
        if (v > out.d) { out.mat = b.mat; out.sid = owner; out.curved = true; }
        out.d = nd;
      } else if (v > out.d) { out.d = v; out.mat = b.mat; out.sid = owner; out.curved = b.shape !== 'box' && b.shape !== 'stairs'; }
    } else if (sd < out.d) {
      out.d = sd;
      if (b.room >= 0 && sd < 0) out.room = roomBase + b.room;
    }
  }
}

// One brush of a recipe, expanded for one storey offset, into the compiled
// form `sdBrush` reads: half sizes, trig for the turns, a conservative
// bounding box, the material's index.
function compileOne(src, dz, parity, matOf, rooms, warn) {
  const at = src.at.slice(); at[2] += dz;
  const size = src.size.slice();
  let rot = src.rot || 0;
  if (src.alternate && parity) { at[0] = -at[0]; at[1] = -at[1]; rot += 180; }
  const b = {
    op: src.op || 'add', shape: src.shape, c: at, h: [size[0] / 2, size[1] / 2, size[2] / 2],
    mat: src.op === 'cut' ? 0 : matOf(src.mat), blend: src.blend || 0, room: -1,
    axis: src.axis || 'u', steps: src.steps || 10, dir: src.dir || 'n', clip: src.clip || null,
  };
  if (rot % 360) b.rot = [Math.cos(rot * RAD), Math.sin(rot * RAD)];
  if (src.tilt) b.tilt = [Math.cos(src.tilt * RAD), Math.sin(src.tilt * RAD)];
  if (src.pitch) b.pitch = [Math.cos(src.pitch * RAD), Math.sin(src.pitch * RAD)];
  const turned = b.rot || b.tilt || b.pitch;
  const m = b.blend + 0.05;
  let ext;
  if (turned) { const r = Math.hypot(b.h[0], b.h[1], b.h[2]); ext = [r, r, r]; } else ext = b.h;
  b.bb = [at[0] - ext[0] - m, at[0] + ext[0] + m, at[1] - ext[1] - m, at[1] + ext[1] + m, at[2] - ext[2] - m, at[2] + ext[2] + m];
  if (src.room && b.op === 'cut') { b.room = rooms.length; rooms.push({ bb: b.bb.slice() }); }
  if (b.op === 'add' && b.shape !== 'stairs' && (size[0] < 0.3 || size[1] < 0.3 || size[2] < 0.3)) warn(`${src.shape} thinner than 0.3 m will alias on a 0.22 m lattice`);
  return b;
}

// A window is an OPENING onto the room: the hole through the wall and
// nothing in it, so what is seen through it is the inside, lamp lit. `face`
// says which wall it is in, so the same size reads as width, depth, height
// on every side. (`lit` is kept for a recipe that wants a pane after all:
// then it is two brushes, the hole and a pane of glass, dark or glowing.)
function expandWindow(src, index, seed, hash) {
  const side = src.face === 'e' || src.face === 'w';
  const size = side ? [src.size[1], src.size[0], src.size[2]] : src.size.slice();
  const hole = { op: 'cut', shape: 'box', at: src.at, size, each: src.each, alternate: src.alternate };
  if (src.pane === undefined) return [hole];
  const pane = side ? [0.3, src.size[0], src.size[2]] : [src.size[0], 0.3, src.size[2]];
  const lit = src.lit === undefined ? hash(index, seed) > 0.45 : !!src.lit;
  return [hole, { op: 'add', shape: 'box', mat: lit ? 'lit' : 'glass', at: src.at, size: pane, each: src.each, alternate: src.alternate }];
}

// Compile a recipe for a building of `storeys` storeys: every brush expanded
// for the storeys it is repeated over (`each`: storey, upper, flight or top),
// mirrored through the centre on odd storeys where it says `alternate`, so
// a flight is on the west wall rising north and then the east wall rising
// south, windows and doors expanded, and the rooms and lamps listed. The
// result is what the field samples and the page places.
export function compile(recipe, storeys, seed, hash) {
  const T = recipe.storey || 3.0, S = storeys;
  const rooms = [], brushes = [], lamps = [], warnings = [];
  const warn = (w) => { if (!warnings.includes(w)) warnings.push(w); };
  const matOf = (name) => { const m = MAT[name || 'concrete']; if (m === undefined) warn(`unknown material ${name}`); return m === undefined ? MAT.concrete : m; };
  let wi = 0;
  const list = [];
  for (const src of recipe.brushes) {
    if (src.shape === 'window') list.push(...expandWindow(src, wi++, seed, hash)); else list.push(src);
  }
  for (const src of list) {
    const reps = [];
    switch (src.each) {
      case 'storey': for (let i = 0; i < S; i++) reps.push([i * T, i & 1]); break;
      case 'upper': for (let f = 1; f < S; f++) reps.push([f * T, (f - 1) & 1]); break;
      case 'flight': for (let i = 0; i < S - 1; i++) reps.push([i * T, i & 1]); break;
      case 'top': reps.push([S * T, S & 1]); break;
      default: reps.push([0, 0]);
    }
    for (const [dz, parity] of reps) {
      const b = compileOne(src, dz, parity, matOf, rooms, warn);
      brushes.push(b);
      if (b.op === 'add' && b.mat === MAT.lamp) lamps.push([b.c[0], b.c[1], b.c[2], Math.max(recipe.footprint[0], recipe.footprint[1]) * 0.8 + 1.5]);
    }
  }
  const bbox = [Infinity, -Infinity, Infinity, -Infinity, Infinity, -Infinity];
  for (const b of brushes) {
    if (b.op !== 'add') continue;
    bbox[0] = Math.min(bbox[0], b.bb[0]); bbox[1] = Math.max(bbox[1], b.bb[1]);
    bbox[2] = Math.min(bbox[2], b.bb[2]); bbox[3] = Math.max(bbox[3], b.bb[3]);
    bbox[4] = Math.min(bbox[4], b.bb[4]); bbox[5] = Math.max(bbox[5], b.bb[5]);
  }
  return { brushes, rooms, lamps, bbox, warnings, storeys: S, storey: T, door: recipe.door || 0, footprint: recipe.footprint };
}

// A single brush as a structure of its own, which is what a sculpt edit
// and a piece of street are.
export function single(src, matName) {
  const rooms = [];
  const b = compileOne(Object.assign({ mat: matName }, src), 0, 0, (n) => MAT[n] === undefined ? MAT.concrete : MAT[n], rooms, () => {});
  const bb = b.bb.slice();
  return { brushes: [b], rooms, lamps: b.op === 'add' && b.mat === MAT.lamp ? [[b.c[0], b.c[1], b.c[2], 4.5]] : [], bbox: bb, warnings: [], storeys: 0, storey: 3, door: 0, footprint: [0, 0] };
}
