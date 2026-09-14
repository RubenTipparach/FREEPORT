// What the two terrain mockups share, so the only thing that differs between
// their pictures is the mesher.
//
// The field is an EXACT port of `crates/freeport_core/src/field.rs`: the same
// lattice hash on wrapping 32 bit arithmetic (`Math.imul` and `>>> 0` are what
// make a JS number behave as a u32), the same smoothstep, the same octave
// rule, so a seed here is the seed there and a mockup approved in the browser
// is the ground the game will march. The material is one triplanar PBR shader
// over the baked terrain sets in `assets/textures/terrain`, blended by slope
// and height, and both meshers wear it.

// three.js by absolute URL and no import map: an import map has to be the
// first module related thing on a page, and a page published as an artifact
// is wrapped by a runtime that may have loaded one already. The orbit is our
// own forty lines rather than the addon, for the same reason: the addon
// imports the bare name 'three', which only an import map can resolve.
import * as THREE from 'https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js';

export { THREE };

// Drag to orbit a target, wheel to zoom, on the pointer events alone.
class Orbit {
  constructor(camera, canvas) {
    this.camera = camera; this.canvas = canvas;
    this.target = new THREE.Vector3(); this.theta = 0.6; this.phi = 1.1; this.dist = 150;
    this.minDistance = 0.5; this.maxDistance = 1200;
    this.enableDamping = false; this.dampingFactor = 0.08;
    let drag = null;
    canvas.addEventListener('pointerdown', (e) => { drag = { x: e.clientX, y: e.clientY }; canvas.setPointerCapture(e.pointerId); });
    canvas.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y; drag = { x: e.clientX, y: e.clientY };
      this.theta -= dx * 0.005; this.phi = Math.min(Math.PI - 0.05, Math.max(0.05, this.phi - dy * 0.005));
    });
    const up = () => { drag = null; };
    canvas.addEventListener('pointerup', up); canvas.addEventListener('pointercancel', up);
    canvas.addEventListener('wheel', (e) => {
      e.preventDefault();
      // Through exp, so a wheel that reports pixels rather than lines cannot make the distance negative.
      const lines = e.deltaMode === 1 ? e.deltaY : e.deltaY / 40;
      this.dist = Math.min(this.maxDistance, Math.max(this.minDistance, this.dist * Math.exp(Math.max(-3, Math.min(3, lines)) * 0.08)));
    }, { passive: false });
  }
  // Take the spherical state off wherever the camera has been put.
  sync() {
    const d = this.camera.position.clone().sub(this.target);
    this.dist = Math.min(this.maxDistance, Math.max(this.minDistance, d.length() || 1));
    this.phi = Math.acos(Math.min(1, Math.max(-1, d.y / this.dist)));
    this.theta = Math.atan2(d.x, d.z);
  }
  update() {
    const s = Math.sin(this.phi);
    this.camera.position.set(
      this.target.x + this.dist * s * Math.sin(this.theta),
      this.target.y + this.dist * Math.cos(this.phi),
      this.target.z + this.dist * s * Math.cos(this.theta));
    this.camera.lookAt(this.target);
  }
}

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

// A planet, `field::Planet`: positive inside the rock, negative in the air.
export class Planet {
  constructor(o) {
    this.radius = o.radius; this.relief = o.relief; this.lumps = o.lumps;
    this.octaves = o.octaves; this.overhang = o.overhang; this.ledge = o.ledge; this.seed = o.seed >>> 0;
  }
  at(x, y, z) {
    const r = Math.sqrt(x * x + y * y + z * z);
    if (r === 0) return this.radius;
    const dx = x / r, dy = y / r, dz = z / r;
    const surface = (fbm3(dx * this.lumps, dy * this.lumps, dz * this.lumps, this.seed, this.octaves) * 2 - 1) * this.relief * 0.5;
    let carve = 0;
    if (this.overhang > 0 && this.ledge > 0) {
      carve = (noise3(x / this.ledge, y / this.ledge, z / this.ledge, (this.seed + 0x9E37) >>> 0) - 0.5) * this.overhang;
    }
    return this.radius + surface - r + carve;
  }
}

// The one planetoid both mockups show. Forty eight metres is small enough to
// mesh whole in a browser and big enough that a walker on it sees a horizon.
export const PLANET = { radius: 48, relief: 20, lumps: 4.0, octaves: 7, overhang: 7.0, ledge: 7.0, seed: 7 };

// ------------------------------------------------------------ the shader --

const VERT = /* glsl */`
  varying vec3 vPos;
  varying vec3 vNrm;
  void main() {
    vPos = (modelMatrix * vec4(position, 1.0)).xyz;
    vNrm = normalize(mat3(modelMatrix) * normal);
    gl_Position = projectionMatrix * viewMatrix * vec4(vPos, 1.0);
  }
`;

// Three sets, blended by slope and by height, sampled on three planes each.
// Lighting is a sun and a dim sky, GGX for the highlight, no fog: this is
// vacuum. The tone mapping and colour space includes are three.js's own so
// the picture goes through the same ACES curve a StandardMaterial would.
const FRAG = /* glsl */`
  precision highp float;
  varying vec3 vPos;
  varying vec3 vNrm;
  uniform vec3 uSun;
  uniform vec3 uCentre;
  uniform float uRadius;
  uniform float uTexScale;
  uniform float uTextured;
  uniform float uSnowLine;
  // Albedo, normal (OpenGL, green up) and ORM (occlusion red, roughness
  // green, metallic blue), the layout Material Maker exports for Godot and
  // the glTF one Bevy reads.
  uniform sampler2D tRockC, tRockN, tRockR;
  uniform sampler2D tGroundC, tGroundN, tGroundR;
  uniform sampler2D tSnowC, tSnowN, tSnowR;
  uniform vec3 uRockFlat, uGroundFlat, uSnowFlat;

  vec3 triW(vec3 n) { vec3 w = pow(abs(n), vec3(4.0)); return w / (w.x + w.y + w.z); }

  vec4 tri(sampler2D t, vec3 p, vec3 w) {
    return texture2D(t, p.yz) * w.x + texture2D(t, p.xz) * w.y + texture2D(t, p.xy) * w.z;
  }

  // Whiteout blend of a tangent space normal map sampled on three planes.
  vec3 triN(sampler2D t, vec3 p, vec3 w, vec3 n) {
    vec3 tx = texture2D(t, p.yz).xyz * 2.0 - 1.0;
    vec3 ty = texture2D(t, p.xz).xyz * 2.0 - 1.0;
    vec3 tz = texture2D(t, p.xy).xyz * 2.0 - 1.0;
    tx = vec3(tx.xy + n.zy, abs(tx.z) * n.x);
    ty = vec3(ty.xy + n.xz, abs(ty.z) * n.y);
    tz = vec3(tz.xy + n.xy, abs(tz.z) * n.z);
    return normalize(tx.zyx * w.x + ty.xzy * w.y + tz.xyz * w.z);
  }

  float ggx(float nh, float a) { float a2 = a * a; float d = nh * nh * (a2 - 1.0) + 1.0; return a2 / (3.14159 * d * d); }

  void main() {
    vec3 n = normalize(vNrm);
    vec3 up = normalize(vPos - uCentre);
    float slope = 1.0 - clamp(dot(n, up), 0.0, 1.0);
    float height = length(vPos - uCentre) - uRadius;
    float wRock = smoothstep(0.30, 0.55, slope);
    float wSnow = smoothstep(uSnowLine - 1.5, uSnowLine + 1.5, height) * (1.0 - wRock);
    float wGround = (1.0 - wRock) * (1.0 - wSnow);
    wRock = 1.0 - wSnow - wGround;

    vec3 p = vPos / uTexScale;
    vec3 w = triW(n);
    vec3 albedo; float rough; vec3 nm; float ao = 1.0;
    if (uTextured > 0.5) {
      albedo = tri(tRockC, p, w).rgb * wRock + tri(tGroundC, p, w).rgb * wGround + tri(tSnowC, p, w).rgb * wSnow;
      vec3 ormRock = tri(tRockR, p, w).rgb, ormGround = tri(tGroundR, p, w).rgb, ormSnow = tri(tSnowR, p, w).rgb;
      vec3 orm = ormRock * wRock + ormGround * wGround + ormSnow * wSnow;
      rough = orm.g;
      ao = orm.r;
      nm = normalize(triN(tRockN, p, w, n) * wRock + triN(tGroundN, p, w, n) * wGround + triN(tSnowN, p, w, n) * wSnow);
    } else {
      albedo = uRockFlat * wRock + uGroundFlat * wGround + uSnowFlat * wSnow;
      rough = 0.85; nm = n;
    }
    vec3 v = normalize(cameraPosition - vPos);
    vec3 l = normalize(uSun);
    vec3 h = normalize(l + v);
    float nl = max(dot(nm, l), 0.0);
    float nh = max(dot(nm, h), 0.0);
    float nv = max(dot(nm, v), 0.001);
    float a = max(rough * rough, 0.02);
    float f0 = 0.04;
    float fres = f0 + (1.0 - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    float spec = ggx(nh, a) * fres / (4.0 * nv + 0.5);
    vec3 sunColour = vec3(1.0, 0.96, 0.9) * 3.2;
    // A vacuum has no sky, so the fill is the planet's own bounce and the
    // stars: low, cool, and stronger on the faces that look out.
    float skyish = 0.5 + 0.5 * dot(nm, up);
    vec3 fill = mix(vec3(0.05, 0.05, 0.07), vec3(0.11, 0.12, 0.16), skyish);
    vec3 colour = albedo * (sunColour * nl + fill * ao) + sunColour * spec * nl;
    gl_FragColor = vec4(colour, 1.0);
    #include <tonemapping_fragment>
    #include <colorspace_fragment>
  }
`;

// Find where the terrain textures are: beside the page when published as an
// artifact, in the repository's assets when opened from the checkout.
async function textureBase() {
  // Probed with an image rather than a fetch, because a fetch refuses a
  // file:// page and an image does not, and a mockup opened straight off the
  // checkout is the common case.
  const exists = (url) => new Promise((ok) => { const i = new Image(); i.onload = () => ok(true); i.onerror = () => ok(false); i.src = url; });
  for (const base of ['textures/', '../../assets/textures/terrain/']) {
    if (await exists(base + 'basalt_albedo.png')) return base;
  }
  return null;
}

function loadTex(loader, url, srgb) {
  return new Promise((resolve) => {
    loader.load(url, (t) => {
      t.wrapS = t.wrapT = THREE.RepeatWrapping;
      t.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
      t.anisotropy = 8;
      resolve(t);
    }, undefined, () => resolve(null));
  });
}

// The material, with its textures loaded if they can be found. `textured`
// false is the geometry alone, which is what a mesher is judged on.
export async function terrainMaterial(renderer) {
  const base = await textureBase();
  const loader = new THREE.TextureLoader();
  const sets = { rock: 'basalt', ground: 'regolith', snow: 'ice' };
  const uniforms = {
    uSun: { value: new THREE.Vector3(0.55, 0.7, 0.45).normalize() },
    uCentre: { value: new THREE.Vector3(0, 0, 0) },
    uRadius: { value: PLANET.radius },
    uTexScale: { value: 5.0 },
    uTextured: { value: base ? 1.0 : 0.0 },
    uSnowLine: { value: 5.0 },
    uRockFlat: { value: new THREE.Color(0.30, 0.28, 0.27) },
    uGroundFlat: { value: new THREE.Color(0.52, 0.46, 0.38) },
    uSnowFlat: { value: new THREE.Color(0.86, 0.90, 0.95) },
  };
  const blank = new THREE.DataTexture(new Uint8Array([255, 220, 0, 255]), 1, 1);
  blank.needsUpdate = true;
  for (const [role, name] of Object.entries(sets)) {
    const cap = role[0].toUpperCase() + role.slice(1);
    uniforms['t' + cap + 'C'] = { value: base ? await loadTex(loader, base + name + '_albedo.png', true) : blank };
    uniforms['t' + cap + 'N'] = { value: base ? await loadTex(loader, base + name + '_normal.png', false) : blank };
    uniforms['t' + cap + 'R'] = { value: base ? await loadTex(loader, base + name + '_orm.png', false) : blank };
    if (!uniforms['t' + cap + 'C'].value) { uniforms['t' + cap + 'C'].value = blank; uniforms.uTextured.value = 0; }
    if (!uniforms['t' + cap + 'N'].value) uniforms['t' + cap + 'N'].value = blank;
    if (!uniforms['t' + cap + 'R'].value) uniforms['t' + cap + 'R'].value = blank;
  }
  if (renderer) {
    const aniso = renderer.capabilities.getMaxAnisotropy();
    for (const u of Object.values(uniforms)) if (u.value && u.value.isTexture) u.value.anisotropy = aniso;
  }
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG });
  mat.hasTextures = uniforms.uTextured.value > 0.5;
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
  const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 4000);
  camera.position.set(90, 60, 110);
  const controls = new Orbit(camera, canvas);
  controls.sync();

  // Stars: points on a far shell, which survive any zoom a texel would not.
  const n = 2600, pos = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const u = hash3(i, 1, 2, 11) * 2 - 1, phi = hash3(i, 3, 4, 11) * Math.PI * 2, r = 1800;
    const s = Math.sqrt(1 - u * u);
    pos[i * 3] = r * s * Math.cos(phi); pos[i * 3 + 1] = r * u; pos[i * 3 + 2] = r * s * Math.sin(phi);
  }
  const stars = new THREE.Points(
    new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(pos, 3)),
    new THREE.PointsMaterial({ color: 0xcfd8ff, size: 1.6, sizeAttenuation: false }));
  scene.add(stars);

  function resize() {
    const w = canvas.clientWidth, h = canvas.clientHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }
  window.addEventListener('resize', resize);
  resize();
  return { renderer, scene, camera, controls, resize };
}

// Two places to stand: in orbit, and on the ground at the sub-solar point,
// eye height off the surface.
export function viewOrbit(controls, camera) {
  controls.target.set(0, 0, 0);
  camera.position.set(90, 60, 110);
  controls.sync();
  controls.update();
}

export function viewSurface(controls, camera, planet, heightAt) {
  const d = new THREE.Vector3(0.55, 0.7, 0.45).normalize();
  const h = heightAt(d) + 1.7;
  const eye = d.clone().multiplyScalar(h);
  camera.position.copy(eye);
  // Look along the ground: a tangent, ten metres out and a little down.
  const tangent = new THREE.Vector3(0, 1, 0).cross(d).normalize();
  controls.target.copy(eye).addScaledVector(tangent, 10).addScaledVector(d, -1.5);
  controls.sync();
  controls.update();
}

// The radius at which the field first turns to rock coming in from space,
// along a direction: what a column of ground is high, and what a walker
// stands on.
export function surfaceRadius(planet, dir) {
  const top = planet.radius + planet.relief * 0.6 + planet.overhang;
  const bottom = planet.radius - planet.relief * 0.6 - planet.overhang;
  let r = top, last = top;
  // Half metre steps and then twelve halvings: the walk finds the crossing
  // and the bisection places it, so the step only has to be finer than the
  // thinnest ledge worth a column.
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

// ---------------------------------------------------------------- the hud --

export function hud(el, rows) {
  el.innerHTML = rows.map(([k, v]) => `<div class="row"><span>${k}</span><b>${v}</b></div>`).join('');
}

export function fmt(n) { return n.toLocaleString('en-US'); }
