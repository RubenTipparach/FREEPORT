// What the two terrain mockups share, so the only thing that differs between
// their pictures is the mesher.
//
// The field is an EXACT port of `crates/freeport_core/src/field.rs`: the same
// lattice hash on wrapping 32 bit arithmetic (`Math.imul` and `>>> 0` are what
// make a JS number behave as a u32), the same smoothstep, the same octave
// rule, so a seed here is the seed there. On top of it the mockups add what
// a town needs and the core does not have yet: a sea level, sites where the
// ground is levelled for a town, a town plan (lots and streets on a local
// grid), one triplanar PBR shader over the baked terrain sets, and a first
// person WALKER, which is the thing the two meshers are actually judged by.
//
// three.js by absolute URL and no import map: an import map has to be the
// first module related thing on a page, and a page published as an artifact
// is wrapped by a runtime that may have loaded one already. The orbit is our
// own forty lines rather than the addon, for the same reason: the addon
// imports the bare name 'three', which only an import map can resolve.

import * as THREE from 'https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js';

export { THREE };

// ------------------------------------------------------------- the field --

function u32mul(a, b) { return Math.imul(a, b) >>> 0; }

// A lattice hash in 0..1, bit for bit `field::hash3`.
export function hash3(x, y, z, seed) {
  let h = (u32mul(x >>> 0, 0x8DA6B343) ^ u32mul(y >>> 0, 0xD8163841)
    ^ u32mul(z >>> 0, 0xCB1AB31F) ^ u32mul(seed >>> 0, 0x9E3779B9)) >>> 0;
  h = (h ^ (h >>> 15)) >>> 0;
  h = u32mul(h, 0x2C1B3C6D);
  h = (h ^ (h >>> 12)) >>> 0;
  h = u32mul(h, 0x297A2D39);
  h = (h ^ (h >>> 15)) >>> 0;
  return h / 4294967296;
}

function smooth(t) { return t * t * (3 - 2 * t); }
function lerp(a, b, t) { return a + (b - a) * t; }
function smoothstep(a, b, t) { const k = Math.min(1, Math.max(0, (t - a) / (b - a))); return k * k * (3 - 2 * k); }

// Value noise on the integer lattice, in 0..1, `field::noise3`.
export function noise3(px, py, pz, seed) {
  const fx = Math.floor(px), fy = Math.floor(py), fz = Math.floor(pz);
  const tx = smooth(px - fx), ty = smooth(py - fy), tz = smooth(pz - fz);
  const c = (dx, dy, dz) => hash3(fx + dx, fy + dy, fz + dz, seed);
  const x00 = lerp(c(0, 0, 0), c(1, 0, 0), tx);
  const x10 = lerp(c(0, 1, 0), c(1, 1, 0), tx);
  const x01 = lerp(c(0, 0, 1), c(1, 0, 1), tx);
  const x11 = lerp(c(0, 1, 1), c(1, 1, 1), tx);
  return lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz);
}

// Fractal sum, `field::fbm3`: each octave doubles the frequency and halves the weight.
export function fbm3(px, py, pz, seed, octaves) {
  let total = 0, amp = 1, norm = 0, freq = 1;
  for (let i = 0; i < Math.max(1, octaves); i++) {
    total += amp * noise3(px * freq, py * freq, pz * freq, (seed + i) >>> 0);
    norm += amp; amp *= 0.5; freq *= 2;
  }
  return total / norm;
}

// A planet, `field::Planet`, plus the two things a town needs: a sea, and
// SITES where the surface is levelled to one height (and the volumetric term
// faded out, or a plateau would still undercut). `surface(dir)` is the relief
// alone, in metres above the mean radius, which is what a column mesher and
// a walker both ask for.
export class Planet {
  constructor(o) {
    this.radius = o.radius; this.relief = o.relief; this.lumps = o.lumps;
    this.octaves = o.octaves; this.overhang = o.overhang; this.ledge = o.ledge; this.seed = o.seed >>> 0;
    this.sea = o.sea;          // radius of the sea surface, metres
    this.shore = o.shore;      // metres above the sea the sand reaches
    this.sites = [];           // { dir: [x,y,z], h: metres above radius, r: metres across }
  }
  // Relief at a direction, metres above the mean radius, sites applied.
  surface(dx, dy, dz) {
    // On a levelled site the relief is the site's height and nothing else,
    // so the noise is not asked: a town marched at 0.22 m is a million
    // samples, nearly all of them on the level.
    for (const site of this.sites) if (this.siteWeight(site, dx, dy, dz) >= 1) return { s: site.h, keep: 0 };
    let s = (fbm3(dx * this.lumps, dy * this.lumps, dz * this.lumps, this.seed, this.octaves) * 2 - 1) * this.relief * 0.5;
    let keep = 1;
    for (const site of this.sites) {
      const w = this.siteWeight(site, dx, dy, dz);
      if (w > 0) { s = lerp(s, site.h, w); keep *= 1 - w; }
    }
    return { s, keep };
  }
  siteWeight(site, dx, dy, dz) {
    const c = Math.min(1, Math.max(-1, dx * site.dir[0] + dy * site.dir[1] + dz * site.dir[2]));
    const dist = Math.acos(c) * this.radius;
    return 1 - smoothstep(site.r * 0.5 - 5, site.r * 0.5 + 6, dist);
  }
  at(x, y, z) {
    const r = Math.sqrt(x * x + y * y + z * z);
    if (r === 0) return this.radius;
    const { s, keep } = this.surface(x / r, y / r, z / r);
    let carve = 0;
    if (this.overhang > 0 && this.ledge > 0) {
      carve = (noise3(x / this.ledge, y / this.ledge, z / this.ledge, (this.seed + 0x9E37) >>> 0) - 0.5) * this.overhang * keep;
    }
    return this.radius + s - r + carve;
  }
}

// The one planet both mockups show: sixty four metres of radius, relief of
// twelve, the sea half a metre under the mean so a little over half of it is
// land, and a sandy shore a metre and a half up the beach. Small enough to
// mesh whole in a browser and big enough that a town of fifty metres is a
// place on it rather than the whole of it.
export const PLANET = { radius: 64, relief: 12, lumps: 5.0, octaves: 6, overhang: 2.5, ledge: 6.0, seed: 7, sea: 63.5, shore: 1.6 };

// The radius at which the field first turns to rock coming in from space,
// along a direction: what a column of ground is high, and what a walker
// stands on.
export function surfaceRadius(planet, dir) {
  const top = planet.radius + planet.relief * 0.6 + planet.overhang;
  const bottom = planet.radius - planet.relief * 0.6 - planet.overhang;
  let r = top, last = top;
  for (; r > bottom; r -= 0.5) {
    if (planet.at(dir.x * r, dir.y * r, dir.z * r) > 0) break;
    last = r;
  }
  let lo = r, hi = last;
  for (let i = 0; i < 12; i++) {
    const m = (lo + hi) * 0.5;
    if (planet.at(dir.x * m, dir.y * m, dir.z * m) > 0) lo = m; else hi = m;
  }
  return (lo + hi) * 0.5;
}

// -------------------------------------------------------------- the towns --

// A local frame on the sphere at a direction: east and north tangents.
export function frameAt(dir) {
  const up = dir.clone().normalize();
  const pole = Math.abs(up.y) < 0.9 ? new THREE.Vector3(0, 1, 0) : new THREE.Vector3(1, 0, 0);
  const east = new THREE.Vector3().crossVectors(pole, up).normalize();
  const north = new THREE.Vector3().crossVectors(up, east).normalize();
  return { up, east, north };
}

// Where a town can stand: on land a little above the sea, on ground that is
// nearly level, and not on top of another town. Candidates come off a
// golden angle spiral, and the first one that qualifies is by the shore,
// because a port is the town this game is about.
export function planTowns(planet, count, seed) {
  const R = planet.radius, sea = planet.sea;
  const golden = Math.PI * (3 - Math.sqrt(5));
  const cands = [];
  for (let i = 0; i < 600; i++) {
    const y = 1 - 2 * (i + 0.5) / 600, s = Math.sqrt(1 - y * y), a = golden * i + hash3(i, seed, 3, seed) * 0.3;
    const dir = new THREE.Vector3(s * Math.cos(a), y, s * Math.sin(a));
    const h = surfaceRadius(planet, dir) - sea;
    if (h < 1.2 || h > 5.5) continue;
    const { east, north } = frameAt(dir);
    let lo = h, hi = h;
    for (const [e, n] of [[1, 0], [-1, 0], [0, 1], [0, -1], [0.7, 0.7], [-0.7, -0.7]]) {
      const d = dir.clone().addScaledVector(east, e * 12 / R).addScaledVector(north, n * 12 / R).normalize();
      const hh = surfaceRadius(planet, d) - sea; lo = Math.min(lo, hh); hi = Math.max(hi, hh);
    }
    if (hi - lo > 2.5) continue;
    cands.push({ dir, h, spread: hi - lo });
  }
  cands.sort((a, b) => a.h - b.h);
  const towns = [];
  for (const c of cands) {
    if (towns.length >= count) break;
    if (towns.some((t) => t.dir.dot(c.dir) > Math.cos(70 / R))) continue;
    towns.push(c);
  }
  return towns.map((c, i) => layTown(planet, c, i, seed));
}

// A town on a local grid: blocks 10 m across with 4 m streets, up to a
// radius of 26 m, a lot per block, taller near the middle, a few blocks
// left as plazas. The first town is the port and gets a long shed on its
// waterfront block.
function layTown(planet, site, index, seed) {
  const R = planet.radius;
  const { east, north } = frameAt(site.dir);
  const pitch = 14, block = 10, street = 4, radius = 26;
  const lots = [], streets = [];
  const n = Math.floor(radius / pitch);
  for (let i = -n; i <= n; i++) {
    streets.push({ x: i * pitch - block / 2 - street / 2, z: 0, w: street, d: radius * 2 + street, along: 'north' });
    streets.push({ x: 0, z: i * pitch - block / 2 - street / 2, w: radius * 2 + street, d: street, along: 'east' });
  }
  for (let i = -n; i <= n; i++) for (let j = -n; j <= n; j++) {
    const cx = i * pitch, cz = j * pitch;
    if (Math.hypot(cx, cz) > radius) continue;
    const k = hash3(i, j, index, seed);
    if (k < 0.15) continue;
    const near = 1 - Math.hypot(cx, cz) / radius;
    const w = 5 + Math.floor(hash3(i, j, 1, seed) * 4), d = 5 + Math.floor(hash3(i, j, 2, seed) * 4);
    const storeys = 1 + Math.floor((hash3(i, j, 3, seed) * 0.6 + near * 0.6) * 4.5);
    lots.push({ x: cx + (hash3(i, j, 4, seed) - 0.5) * (block - w), z: cz + (hash3(i, j, 5, seed) - 0.5) * (block - d), w, d, storeys, kind: hash3(i, j, 6, seed) < 0.3 ? 'gable' : 'flat' });
  }
  const plan = { dir: site.dir, east, north, h: site.h + planet.sea - R, radius, lots, streets, index };
  planet.sites.push({ dir: [site.dir.x, site.dir.y, site.dir.z], h: plan.h, r: radius * 2 + 12 });
  return plan;
}

// A lot's own place and frame on the sphere: its centre direction, and the
// east and north there, so a building stands plumb on its own patch of
// ground rather than on the town centre's.
export function lotFrame(planet, town, x, z) {
  const dir = town.dir.clone().addScaledVector(town.east, x / planet.radius).addScaledVector(town.north, z / planet.radius).normalize();
  const f = frameAt(dir);
  // Keep the town's heading: east at the lot is the town's east projected.
  const east = town.east.clone().addScaledVector(dir, -town.east.dot(dir)).normalize();
  const north = new THREE.Vector3().crossVectors(dir, east).normalize();
  return { dir, up: f.up, east, north, base: planet.radius + town.h };
}

// ------------------------------------------------------------ the shader --

// The tag and the material are FLAT: a triangle takes its last vertex's,
// unblended, so concrete meets rock on a line and never as a gradient. The
// mesher orders every triangle so its last vertex is the one whose material
// should win.
const VERT = /* glsl */`
  attribute float tag;
  attribute float base;
  attribute float mat;
  varying vec3 vPos;
  varying vec3 vNrm;
  varying vec2 vUv;
  flat varying float vTag;
  flat varying float vMat;
  varying float vBase;
  void main() {
    vPos = (modelMatrix * vec4(position, 1.0)).xyz;
    vNrm = normalize(mat3(modelMatrix) * normal);
    vUv = uv; vTag = tag; vBase = base; vMat = mat;
    gl_Position = projectionMatrix * viewMatrix * vec4(vPos, 1.0);
  }
`;

// How many lamps the shader can hold. A uniform array is a fixed size in
// GLSL, so this is the ceiling on lit rooms in a scene, not a budget.
export const MAX_LAMPS = 96;

// Five sets blended by slope and height, sampled on three planes each: rock
// on the steep, sand up to the shore line, grass above it, ice above the snow
// line. Anything with a MATERIAL is built: 1 concrete, 2 plate, 3 glass, 4 a
// lamp (glows, lit by nothing), 5 a lit pane, 6 a street (concrete, darker).
// It wears its maps on the mesh's own UVs (the hex page lays them in the
// town's frame so the panels meet the blocks) or, on the marched page where
// a building is the field and has no UVs, on three planes in a LOCAL frame,
// east, north and up at the fragment, so a wall's panels run level and
// plumb whatever the planet's axes do, and up is measured from the tag's
// base so the seams meet the floors. The TAG is the light: 0 and 1 are the
// sun, 2 the sun on a street, 3 INSIDE, where the sun does not reach and
// the light is the room's lamps, 4 a lamp. A hex wall gets its panes from a
// hash (uShaderWindows); a marched wall has them as glass brushes. Outside,
// lighting is a sun and a dim sky, GGX for the highlight, no fog: vacuum.
const FRAG = /* glsl */`
  precision highp float;
  precision highp sampler2DArray;
  varying vec3 vPos;
  varying vec3 vNrm;
  varying vec2 vUv;
  flat varying float vTag;
  flat varying float vMat;
  varying float vBase;
  uniform float uBuiltUv;
  uniform float uShaderWindows;
  uniform float uDebug;
  uniform vec4 uLamps[${MAX_LAMPS}];
  uniform int uLampCount;
  uniform vec3 uSun;
  uniform vec3 uCentre;
  uniform float uRadius;
  uniform float uSea;
  uniform float uShore;
  uniform float uTexScale;
  uniform float uTextured;
  uniform float uSnowLine;
  // The material sets are three ARRAY textures, a layer a set, because a
  // set is three maps and six sets as eighteen samplers is past the sixteen
  // a fragment shader is promised: it drew on swiftshader and not on a GPU.
  uniform highp sampler2DArray tC, tN, tR;
  uniform vec3 uRockFlat, uSandFlat, uGrassFlat, uSnowFlat, uConcFlat, uPlateFlat;
  const float L_ROCK = 0.0, L_SAND = 1.0, L_GRASS = 2.0, L_SNOW = 3.0, L_CONC = 4.0, L_PLATE = 5.0;

  vec3 triW(vec3 n) { vec3 w = pow(abs(n), vec3(4.0)); return w / (w.x + w.y + w.z); }
  vec4 tri(sampler2DArray t, float l, vec3 p, vec3 w) { return texture(t, vec3(p.yz, l)) * w.x + texture(t, vec3(p.xz, l)) * w.y + texture(t, vec3(p.xy, l)) * w.z; }
  vec3 triN(sampler2DArray t, float l, vec3 p, vec3 w, vec3 n) {
    vec3 tx = texture(t, vec3(p.yz, l)).xyz * 2.0 - 1.0;
    vec3 ty = texture(t, vec3(p.xz, l)).xyz * 2.0 - 1.0;
    vec3 tz = texture(t, vec3(p.xy, l)).xyz * 2.0 - 1.0;
    tx = vec3(tx.xy + n.zy, abs(tx.z) * n.x);
    ty = vec3(ty.xy + n.xz, abs(ty.z) * n.y);
    tz = vec3(tz.xy + n.xy, abs(tz.z) * n.z);
    return normalize(tx.zyx * w.x + ty.xzy * w.y + tz.xyz * w.z);
  }
  // A built material's maps, on the mesh's UVs or on three local planes. The
  // normal comes back in the frame it was read in; the caller turns it out.
  void builtMaps(float l, vec2 uv, vec3 lp, vec3 lw, vec3 ln, out vec3 alb, out vec3 orm, out vec3 nml) {
    if (uBuiltUv > 0.5) {
      alb = texture(tC, vec3(uv, l)).rgb; orm = texture(tR, vec3(uv, l)).rgb; nml = texture(tN, vec3(uv, l)).xyz * 2.0 - 1.0;
    } else {
      alb = tri(tC, l, lp, lw).rgb; orm = tri(tR, l, lp, lw).rgb; nml = triN(tN, l, lp, lw, ln);
    }
  }
  float ggx(float nh, float a) { float a2 = a * a; float d = nh * nh * (a2 - 1.0) + 1.0; return a2 / (3.14159 * d * d); }
  float hashf(vec2 p) { vec3 q = fract(vec3(p.xyx) * vec3(443.897, 441.423, 437.195)); q += dot(q, q.yzx + 19.19); return fract((q.x + q.y) * q.z); }

  void main() {
    vec3 n = normalize(vNrm);
    vec3 up = normalize(vPos - uCentre);
    float slope = 1.0 - clamp(dot(n, up), 0.0, 1.0);
    float r = length(vPos - uCentre);
    float above = r - uSea;
    vec3 p = vPos / uTexScale;
    vec3 w = triW(n);
    vec3 albedo; float rough; vec3 nm; float ao = 1.0; vec3 glow = vec3(0.0);
    vec3 dbg = vec3(0.0);

    // The material rides in the low bits and a flag in the eighth: a curved
    // brush is shaded smooth on its vertex normals, a box is shaded FLAT on
    // the triangle's own face, because a box's edges are edges and a normal
    // interpolated across them rounds every corner over a cell.
    int m = int(mod(vMat, 8.0) + 0.5);
    bool curved = vMat >= 8.0;
    bool inside = vTag > 2.5 && vTag < 3.5;
    bool lamp = vTag > 3.5 || m == 4;
    bool built = m > 0 || vTag > 0.5;
    if (built && !curved && uBuiltUv < 0.5) {
      vec3 fn = cross(dFdx(vPos), dFdy(vPos));
      if (dot(fn, fn) > 0.0) { fn = normalize(fn); n = dot(fn, n) < 0.0 ? -fn : fn; }
    }
    if (built) {
      // The local frame: east, north and up at the fragment for the normal,
      // and for the POSITION the arc east and north of a fixed pole, in
      // metres, with up measured from the tag's base, in panels of 3 m.
      // Not the position projected on the frame at the fragment: a point on
      // a sphere projected on its own tangent plane is nought everywhere,
      // and the first cut's panels were float noise.
      vec3 pole = abs(up.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
      vec3 e = normalize(cross(pole, up));
      vec3 nn = cross(up, e);
      vec3 ln = vec3(dot(n, e), dot(n, nn), dot(n, up));
      float lat = asin(clamp(up.y, -1.0, 1.0));
      vec3 lp = vec3(atan(up.z, up.x) * r * cos(lat), lat * r, r - vBase) / 3.0;
      vec3 lw = triW(ln);
      dbg = uDebug > 2.5 ? fract(lp) : lw;
      if (uTextured > 0.5 && m != 3 && m != 5) {
        vec3 alb, orm, nml;
        builtMaps(m == 2 ? L_PLATE : L_CONC, vUv, lp, lw, ln, alb, orm, nml);
        albedo = alb; rough = orm.g; ao = orm.r;
        vec3 bumped;
        if (uBuiltUv > 0.5) {
          // The map's tangent frame is the UV's; a wall's u runs along it and v up it.
          vec3 t1 = normalize(cross(n, abs(n.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0)));
          vec3 t2 = cross(t1, n);
          bumped = normalize(t1 * nml.x + t2 * nml.y + n * nml.z);
        } else {
          bumped = normalize(e * nml.x + nn * nml.y + up * nml.z);
        }
        // At half strength: a lamp a metre off a wall lights every grain of
        // a full strength map from the side, and the panel reads as stipple.
        nm = normalize(mix(n, bumped, 0.5));
      } else { albedo = m == 2 ? uPlateFlat : uConcFlat; rough = 0.85; nm = n; }
      if ((vTag > 1.5 && vTag < 2.5) || m == 6) { albedo *= 0.42; rough = 0.95; }
      if (inside) { albedo *= 1.15; }
      if (m == 3) { albedo = vec3(0.05, 0.06, 0.07); rough = 0.12; nm = n; }
      if (m == 5) { albedo = vec3(0.03); rough = 0.15; nm = n; glow = vec3(1.0, 0.78, 0.5) * 1.6; }
      if (lamp) { albedo = vec3(0.8); glow = vec3(1.0, 0.86, 0.66) * 0.9; }
      if (uShaderWindows > 0.5 && vTag > 0.5 && vTag < 1.5 && slope > 0.5) {
        float storey = (r - vBase) / 3.0;
        float f = fract(storey);
        vec3 t = normalize(cross(n, up));
        float along = dot(vPos, t) / 2.4;
        float a = fract(along);
        if (f > 0.35 && f < 0.78 && a > 0.22 && a < 0.72 && storey > 0.2) {
          float lit = step(0.45, hashf(vec2(floor(storey), floor(along)) + vBase));
          albedo = mix(vec3(0.05, 0.06, 0.07), vec3(0.03), lit); rough = 0.15; nm = n;
          glow = lit * vec3(1.0, 0.78, 0.5) * 1.6;
        }
      }
    } else {
      float wRock = smoothstep(0.34, 0.6, slope);
      float wSand = (1.0 - smoothstep(uShore - 0.5, uShore + 0.6, above)) * (1.0 - wRock);
      float wSnow = smoothstep(uSnowLine - 1.5, uSnowLine + 1.5, above) * (1.0 - wRock) * (1.0 - wSand);
      float wGrass = max(0.0, 1.0 - wRock - wSand - wSnow);
      if (uTextured > 0.5) {
        albedo = tri(tC, L_ROCK, p, w).rgb * wRock + tri(tC, L_SAND, p, w).rgb * wSand + tri(tC, L_GRASS, p, w).rgb * wGrass + tri(tC, L_SNOW, p, w).rgb * wSnow;
        vec3 orm = tri(tR, L_ROCK, p, w).rgb * wRock + tri(tR, L_SAND, p, w).rgb * wSand + tri(tR, L_GRASS, p, w).rgb * wGrass + tri(tR, L_SNOW, p, w).rgb * wSnow;
        rough = orm.g; ao = orm.r;
        nm = normalize(triN(tN, L_ROCK, p, w, n) * wRock + triN(tN, L_SAND, p, w, n) * wSand + triN(tN, L_GRASS, p, w, n) * wGrass + triN(tN, L_SNOW, p, w, n) * wSnow);
      } else {
        albedo = uRockFlat * wRock + uSandFlat * wSand + uGrassFlat * wGrass + uSnowFlat * wSnow; rough = 0.85; nm = n;
      }
      // Wet sand at the water line.
      float wet = 1.0 - smoothstep(0.0, 0.5, above);
      albedo *= 1.0 - 0.35 * wet; rough = mix(rough, 0.35, wet);
    }
    vec3 v = normalize(cameraPosition - vPos);
    float a = max(rough * rough, 0.02);
    float f0 = 0.04;
    float nv = max(dot(nm, v), 0.001);
    vec3 colour;
    if (inside || lamp) {
      // A room: its lamps, warm, falling off as a lamp does, and a little
      // bounce off the walls so a corner is dark and not black.
      vec3 lit = vec3(0.0);
      for (int i = 0; i < ${MAX_LAMPS}; i++) {
        if (i >= uLampCount) break;
        vec3 L = uLamps[i].xyz - vPos;
        float d2 = dot(L, L);
        float reach = uLamps[i].w;
        if (d2 > reach * reach) continue;
        vec3 l = L * inversesqrt(d2);
        float att = (1.0 - d2 / (reach * reach)) / (1.0 + d2 * 0.35);
        vec3 h = normalize(l + v);
        // Wrapped, because a lamp a hand under a ceiling lights the whole
        // ceiling at a grazing angle, and a plain cosine there is every
        // grain of the map lit on one side: gravel, not concrete.
        float nl = max(dot(nm, l) * 0.7 + 0.3, 0.0);
        float fres = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
        float spec = ggx(max(dot(nm, h), 0.0), a) * fres / (4.0 * nv + 0.5);
        lit += vec3(1.0, 0.86, 0.66) * 2.2 * att * (nl + spec * nl);
      }
      colour = albedo * (lit + vec3(0.09, 0.08, 0.07) * ao) + glow;
    } else {
      vec3 l = normalize(uSun);
      vec3 h = normalize(l + v);
      float nl = max(dot(nm, l), 0.0);
      float nh = max(dot(nm, h), 0.0);
      float fres = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
      float spec = ggx(nh, a) * fres / (4.0 * nv + 0.5);
      vec3 sunColour = vec3(1.0, 0.96, 0.9) * 3.2;
      float skyish = 0.5 + 0.5 * dot(nm, up);
      vec3 fill = mix(vec3(0.10, 0.09, 0.08), vec3(0.20, 0.23, 0.28), skyish);
      colour = albedo * (sunColour * nl + fill * ao) + sunColour * spec * nl + glow;
    }
    gl_FragColor = vec4(colour, 1.0);
    if (uDebug > 0.5) {
      vec3 dpole = abs(up.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
      vec3 de = normalize(cross(dpole, up)); vec3 dn = cross(up, de);
      if (uDebug < 1.5) gl_FragColor = vec4(albedo, 1.0);
      else if (uDebug < 2.5) gl_FragColor = vec4(nm * 0.5 + 0.5, 1.0);
      else if (uDebug < 3.5) gl_FragColor = vec4(dbg, 1.0);
      else if (uDebug < 4.5) gl_FragColor = vec4(vMat / 6.0, vTag / 4.0, 0.0, 1.0);
      else if (uDebug < 5.5) gl_FragColor = vec4(fract(vPos / 3.0), 1.0);
      else if (uDebug < 6.5) gl_FragColor = vec4(fract(vec3(dot(vPos, de), dot(vPos, dn), r - vBase) / 3.0), 1.0);
      else gl_FragColor = vec4(de * 0.5 + 0.5, 1.0);
      return;
    }
    #include <tonemapping_fragment>
    #include <colorspace_fragment>
  }
`;

// Find where the terrain textures are: beside the page when published as an
// artifact, in the repository's assets when opened from the checkout.
// Probed with an image rather than a fetch, because a fetch refuses a
// file:// page and an image does not.
export async function textureBase() {
  const exists = (url) => new Promise((ok) => { const i = new Image(); i.onload = () => ok(true); i.onerror = () => ok(false); i.src = url; });
  for (const base of ['textures/', '../../assets/textures/terrain/']) {
    if (await exists(base + 'basalt_albedo.png')) return base;
  }
  return null;
}

export function loadTex(loader, url, srgb) {
  return new Promise((resolve) => {
    loader.load(url, (t) => {
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      t.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
      resolve(t);
    }, undefined, () => resolve(null));
  });
}

export const SUN = new THREE.Vector3(0.55, 0.7, 0.45).normalize();

// The maps of every set, one kind at a time, stacked into one array
// texture with a layer a set, so the shader binds three samplers and not
// three a set. The rows are turned over on the way in, because an image
// is uploaded upside down and pixel data is not, and a normal map's green
// would otherwise point the other way. Without the bakes it is one yellow
// texel a layer and the shader draws its flat colours.
export function stackMaps(texs, real, srgb, aniso) {
  const n = texs.length;
  const size = real ? texs[0].image.width : 1;
  const data = new Uint8Array(size * size * 4 * n);
  if (real) {
    const cv = document.createElement('canvas'); cv.width = size; cv.height = size;
    const ctx = cv.getContext('2d', { willReadFrequently: true });
    for (let l = 0; l < n; l++) {
      ctx.drawImage(texs[l].image, 0, 0, size, size);
      const px = ctx.getImageData(0, 0, size, size).data, row = size * 4;
      for (let y = 0; y < size; y++) data.set(px.subarray(y * row, (y + 1) * row), l * size * size * 4 + (size - 1 - y) * row);
    }
  } else {
    for (let i = 0; i < data.length; i += 4) { data[i] = 255; data[i + 1] = 220; data[i + 2] = 0; data[i + 3] = 255; }
  }
  const t = new THREE.DataArrayTexture(data, size, size, n);
  t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
  t.minFilter = THREE.LinearMipmapLinearFilter; t.magFilter = THREE.LinearFilter;
  t.generateMipmaps = true; t.anisotropy = aniso; t.needsUpdate = true;
  return t;
}


// The terrain material, with its textures loaded if they can be found, and
// the same textures handed back so a page can dress its own meshes in them.
export async function terrainMaterial(renderer) {
  const base = await textureBase();
  const loader = new THREE.TextureLoader();
  const sets = { Rock: 'basalt', Sand: 'dunes', Grass: 'grass', Snow: 'ice', Conc: 'concrete', Plate: 'hull_plate' };
  const uniforms = {
    uSun: { value: SUN.clone() }, uCentre: { value: new THREE.Vector3(0, 0, 0) },
    uRadius: { value: PLANET.radius }, uSea: { value: PLANET.sea }, uShore: { value: PLANET.shore },
    uTexScale: { value: 4.0 }, uTextured: { value: base ? 1.0 : 0.0 }, uSnowLine: { value: 7.0 },
    uRockFlat: { value: new THREE.Color(0.30, 0.28, 0.27) }, uSandFlat: { value: new THREE.Color(0.78, 0.66, 0.45) },
    uGrassFlat: { value: new THREE.Color(0.25, 0.4, 0.14) }, uSnowFlat: { value: new THREE.Color(0.86, 0.90, 0.95) },
    uConcFlat: { value: new THREE.Color(0.6, 0.6, 0.58) }, uPlateFlat: { value: new THREE.Color(0.45, 0.47, 0.5) },
    uBuiltUv: { value: 0.0 }, uShaderWindows: { value: 0.0 }, uDebug: { value: 0.0 },
    uLamps: { value: Array.from({ length: MAX_LAMPS }, () => new THREE.Vector4(0, 0, 0, 0)) },
    uLampCount: { value: 0 },
  };
  const textures = {}, names = Object.values(sets), loaded = [];
  for (const name of names) {
    const c = base ? await loadTex(loader, base + name + '_albedo.png', true) : null;
    const nrm = base ? await loadTex(loader, base + name + '_normal.png', false) : null;
    const orm = base ? await loadTex(loader, base + name + '_orm.png', false) : null;
    if (!c || !nrm || !orm) uniforms.uTextured.value = 0;
    textures[name] = { albedo: c, normal: nrm, orm }; loaded.push([c, nrm, orm]);
  }
  const aniso = renderer ? renderer.capabilities.getMaxAnisotropy() : 1;
  const stacked = uniforms.uTextured.value > 0.5;
  uniforms.tC = { value: stackMaps(loaded.map((l) => l[0]), stacked, true, aniso) };
  uniforms.tN = { value: stackMaps(loaded.map((l) => l[1]), stacked, false, aniso) };
  uniforms.tR = { value: stackMaps(loaded.map((l) => l[2]), stacked, false, aniso) };
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG });
  mat.hasTextures = uniforms.uTextured.value > 0.5;
  mat.textures = textures;
  // Hand the shader its lamps: world position and reach, at most MAX_LAMPS.
  mat.setLamps = (lamps) => {
    const n = Math.min(lamps.length, MAX_LAMPS);
    for (let i = 0; i < n; i++) uniforms.uLamps.value[i].set(lamps[i].x, lamps[i].y, lamps[i].z, lamps[i].w);
    uniforms.uLampCount.value = n;
  };
  return mat;
}

// ------------------------------------------------------------- the scene --

export function makeScene(canvas) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x02040a);
  const camera = new THREE.PerspectiveCamera(60, 1, 0.08, 4000);
  camera.position.set(110, 75, 135);
  const controls = new Orbit(camera, canvas);
  controls.sync();

  // The same sun the terrain shader lights by, for the standard materials.
  const sun = new THREE.DirectionalLight(0xfff4e4, 3.0);
  sun.position.copy(SUN).multiplyScalar(500);
  scene.add(sun);
  // A hemisphere rather than a flat ambient: a wall facing away from the sun
  // is lit by the ground's bounce and the sky's glow, and a flat term lit
  // every south face the same near black. The terrain shader's fill is the
  // same idea in its own words.
  scene.add(new THREE.HemisphereLight(0x8fa4c4, 0x5a4a3a, 1.4));

  // Stars: points on a far shell, which survive any zoom a texel would not.
  const n = 2600, pos = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const u = hash3(i, 1, 2, 11) * 2 - 1, phi = hash3(i, 3, 4, 11) * Math.PI * 2, r = 1800;
    const s = Math.sqrt(1 - u * u);
    pos[i * 3] = r * s * Math.cos(phi); pos[i * 3 + 1] = r * u; pos[i * 3 + 2] = r * s * Math.sin(phi);
  }
  scene.add(new THREE.Points(new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(pos, 3)),
    new THREE.PointsMaterial({ color: 0xcfd8ff, size: 1.6, sizeAttenuation: false })));

  function resize() {
    const w = canvas.clientWidth, h = canvas.clientHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }
  window.addEventListener('resize', resize);
  resize();
  return { renderer, scene, camera, controls, resize, sun };
}

// The sea: a sphere at the sea level, glossy and a little transparent, so
// the shore under it reads through.
export function makeSea(planet) {
  const geo = new THREE.SphereGeometry(planet.sea, 160, 96);
  const mat = new THREE.MeshPhysicalMaterial({ color: 0x0d3550, roughness: 0.06, metalness: 0.0, transparent: true, opacity: 0.82, envMapIntensity: 0.0 });
  return new THREE.Mesh(geo, mat);
}

// Drag to orbit a target, wheel to zoom, on the pointer events alone.
class Orbit {
  constructor(camera, canvas) {
    this.camera = camera; this.canvas = canvas; this.enabled = true;
    this.target = new THREE.Vector3(); this.theta = 0.6; this.phi = 1.1; this.dist = 150;
    this.minDistance = 0.5; this.maxDistance = 1200;
    let drag = null;
    canvas.addEventListener('pointerdown', (e) => { if (!this.enabled) return; drag = { x: e.clientX, y: e.clientY }; canvas.setPointerCapture(e.pointerId); });
    canvas.addEventListener('pointermove', (e) => {
      if (!drag || !this.enabled) return;
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y; drag = { x: e.clientX, y: e.clientY };
      this.theta -= dx * 0.005; this.phi = Math.min(Math.PI - 0.05, Math.max(0.05, this.phi - dy * 0.005));
    });
    const up = () => { drag = null; };
    canvas.addEventListener('pointerup', up); canvas.addEventListener('pointercancel', up);
    canvas.addEventListener('wheel', (e) => {
      if (!this.enabled) return;
      e.preventDefault();
      // Through exp, so a wheel that reports pixels rather than lines cannot make the distance negative.
      const lines = e.deltaMode === 1 ? e.deltaY : e.deltaY / 40;
      this.dist = Math.min(this.maxDistance, Math.max(this.minDistance, this.dist * Math.exp(Math.max(-3, Math.min(3, lines)) * 0.08)));
    }, { passive: false });
  }
  sync() {
    const d = this.camera.position.clone().sub(this.target);
    this.dist = Math.min(this.maxDistance, Math.max(this.minDistance, d.length() || 1));
    this.phi = Math.acos(Math.min(1, Math.max(-1, d.y / this.dist)));
    this.theta = Math.atan2(d.x, d.z);
  }
  update() {
    if (!this.enabled) return;
    const s = Math.sin(this.phi);
    this.camera.up.set(0, 1, 0);
    this.camera.position.set(
      this.target.x + this.dist * s * Math.sin(this.theta),
      this.target.y + this.dist * Math.cos(this.phi),
      this.target.z + this.dist * s * Math.cos(this.theta));
    this.camera.lookAt(this.target);
  }
}

export function viewOrbit(controls, camera) {
  controls.enabled = true;
  controls.target.set(0, 0, 0);
  camera.position.set(110, 75, 135);
  controls.sync();
  controls.update();
}

// ------------------------------------------------------------ the walker --

// A first person walker on a sphere. Its position is a direction and a
// height off the ground; its heading is a tangent vector carried along with
// it and squared to the local up every frame, which is the surface walker
// as a BASIS (CLAUDE.md: a body glued to a surface has a real up and no
// roll). Velocity has acceleration and friction so a step has weight, a
// body radius keeps it off walls, a step height lets it walk up a kerb and
// a jump clears a metre. What the ground IS comes from the page:
//   ground(dir) -> radius of the surface to stand on under a direction
//   resolve(dir, footTop) -> that direction pushed out of anything solid
//     the body overlaps below footTop, so a wall is slid along rather than
//     stuck to: the walker never asks "may I", it asks "where do I end up"
//   ceiling(dir), optional -> the lowest solid above the feet, so a jump
//     under a floor stops at it
// A page's ground answers relative to the walker's FEET (`foot`, a radius):
// the highest support no more than a step above them, which is what lets a
// walker stand on a floor with another floor over it.
// which is exactly the line between the two meshers under test.
export class Walker {
  constructor(camera, canvas, planet, rules) {
    this.camera = camera; this.canvas = canvas; this.planet = planet; this.rules = rules;
    this.eye = 1.7; this.radius = 0.35; this.step = 0.6; this.speed = 5.0; this.run = 8.5;
    this.accel = 28; this.friction = 14; this.jumpV = 5.3; this.gravity = 9.81; this.wade = 1.1;
    this.active = false; this.dir = new THREE.Vector3(0, 1, 0); this.fwd = new THREE.Vector3(1, 0, 0);
    this.pitch = 0; this.h = 0; this.vy = 0; this.vel = new THREE.Vector2(0, 0); this.onGround = true;
    this.foot = Infinity; this.head = 1.85;
    this.keys = new Set(); this.hint = 0; this.stat = '';
    document.addEventListener('keydown', (e) => { if (!this.active) return; this.keys.add(e.code); if (['Space', 'KeyW', 'KeyA', 'KeyS', 'KeyD'].includes(e.code)) e.preventDefault(); });
    document.addEventListener('keyup', (e) => this.keys.delete(e.code));
    document.addEventListener('mousemove', (e) => {
      if (!this.active || document.pointerLockElement !== canvas) return;
      const dx = Math.max(-80, Math.min(80, e.movementX)), dy = Math.max(-80, Math.min(80, e.movementY));
      this.turn(-dx * 0.0022); this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch - dy * 0.0022));
    });
    canvas.addEventListener('click', () => { if (this.active && document.pointerLockElement !== canvas) canvas.requestPointerLock(); });
  }
  turn(a) { this.fwd.applyAxisAngle(this.dir, a); }
  enter(dir, heading) {
    this.active = true; this.dir.copy(dir).normalize();
    this.fwd.copy(heading || new THREE.Vector3(1, 0, 0));
    this.fwd.addScaledVector(this.dir, -this.fwd.dot(this.dir)).normalize();
    this.h = 0; this.vy = 0; this.vel.set(0, 0); this.pitch = -0.05;
    this.foot = Infinity; this.foot = this.rules.ground(this.dir, this);
    this.canvas.requestPointerLock && this.canvas.requestPointerLock();
  }
  exit() { this.active = false; if (document.pointerLockElement === this.canvas) document.exitPointerLock(); }
  update(dt) {
    if (!this.active) return;
    dt = Math.min(dt, 0.05);
    const up = this.dir, R = this.planet.radius;
    this.fwd.addScaledVector(up, -this.fwd.dot(up)).normalize();
    const right = new THREE.Vector3().crossVectors(this.fwd, up).normalize();
    // Wanted velocity in the tangent plane, then accelerate toward it.
    let f = (this.keys.has('KeyW') ? 1 : 0) - (this.keys.has('KeyS') ? 1 : 0);
    let r = (this.keys.has('KeyD') ? 1 : 0) - (this.keys.has('KeyA') ? 1 : 0);
    const len = Math.hypot(f, r) || 1;
    const top = this.keys.has('ShiftLeft') || this.keys.has('ShiftRight') ? this.run : this.speed;
    const want = new THREE.Vector2(f / len * top, r / len * top);
    const gain = this.onGround ? this.accel : this.accel * 0.25;
    this.vel.x += (want.x - this.vel.x) * Math.min(1, gain * dt / top);
    this.vel.y += (want.y - this.vel.y) * Math.min(1, gain * dt / top);
    if (f === 0 && r === 0 && this.onGround) this.vel.multiplyScalar(Math.max(0, 1 - this.friction * dt));
    // Try the step whole, then each axis alone, so a wall is slid along.
    const groundNow = this.rules.ground(this.dir, this);
    const foot = groundNow + this.h;
    const tryStep = (vf, vr) => {
      if (vf === 0 && vr === 0) return null;
      let d = this.dir.clone().addScaledVector(this.fwd, vf * dt / R).addScaledVector(right, vr * dt / R).normalize();
      d = this.rules.resolve(d, foot + this.step, this);
      // A resolved step that goes backwards is a wall square on: not a move.
      if (d.dot(this.dir) >= 1 - 1e-12 && d.distanceTo(this.dir) < 1e-9) return null;
      const g = this.rules.ground(d, this);
      if (g - foot > this.step) return null;
      if (g < this.planet.sea - this.wade) return null;
      // A step is a rise onto ground that can be stood on: a page that knows
      // its slopes refuses a face too steep, or a rail's end is a stair. It
      // is told where the step came from, so it can look past the face.
      if (this.rules.canStand && !this.rules.canStand(d, g, this, this.dir)) return null;
      return { d, g };
    };
    let moved = tryStep(this.vel.x, this.vel.y) || tryStep(this.vel.x, 0) || tryStep(0, this.vel.y);
    if (moved) {
      this.dir.copy(moved.d);
      // Standing on ground that rose a little is a step up. Ground that fell
      // by no more than a step is a step DOWN and the feet stay on it: the
      // first cut went airborne on every downslope and flickered down a
      // plinth's fillet. Further than a step is a fall.
      const drop = foot - moved.g;
      this.h = this.onGround && drop <= this.step ? 0 : Math.max(0, drop);
    } else if (f !== 0 || r !== 0) { this.vel.multiplyScalar(0.5); }
    // Vertical.
    if (this.keys.has('Space') && this.onGround) { this.vy = this.jumpV; this.onGround = false; }
    this.vy -= this.gravity * dt;
    this.h += this.vy * dt;
    if (this.h <= 0) { this.h = 0; this.vy = 0; this.onGround = true; } else { this.onGround = false; }
    const ground = this.rules.ground(this.dir, this);
    if (this.rules.ceiling) {
      const c = this.rules.ceiling(this.dir, this);
      const room = c - (ground + this.h + this.head);
      if (room < 0) { this.h = Math.max(0, this.h + room); if (this.vy > 0) this.vy = 0; }
    }
    this.foot = ground + this.h;
    const pos = this.dir.clone().multiplyScalar(ground + this.h + this.eye);
    this.camera.position.copy(pos);
    this.camera.up.copy(up);
    const look = pos.clone().addScaledVector(this.fwd, Math.cos(this.pitch)).addScaledVector(up, Math.sin(this.pitch));
    this.camera.lookAt(look);
    this.stat = `${(ground - this.planet.sea).toFixed(1)} m above the sea, ${Math.hypot(this.vel.x, this.vel.y).toFixed(1)} m/s${this.onGround ? '' : ', airborne'}${this.rules.where ? ', ' + this.rules.where(this.dir, this) : ''}`;
  }
}

// ---------------------------------------------------------------- the hud --

export function hud(el, rows) {
  el.innerHTML = rows.map(([k, v]) => `<div class="row"><span>${k}</span><b>${v}</b></div>`).join('');
}

export function fmt(n) { return n.toLocaleString('en-US'); }
