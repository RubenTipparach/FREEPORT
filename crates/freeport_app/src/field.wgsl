#define_import_path freeport::field

// The ground, on the GPU: `freeport_core::field`'s `mix3`, `noise3`, `fbm3`
// and `Planet::surface` transcribed to WGSL, so a compute pass that places
// a hundred thousand hex columns and a vertex stage that displaces a level
// of detail triangle both ask the same question the walker asks on the CPU.
//
// What parts company with the core, and it is one thing: a GPU has no f64.
// `mix3` is whole numbers and is EXACT here (WGSL's u32 wraps, which is
// what the core's `wrapping_mul` does), but turning its thirty two bits
// into a number in 0..1 costs a float's bottom eight, so this hash is the
// core's to about one part in 2^24. `hash3_f32` is that rounding written
// in the core and `a_float_hash_is_the_cores_to_a_hundred_millionth` is
// what bounds it: under a hundredth of a millimetre on the harness's 160 m
// relief, which is under anything a renderer can show and well under the
// quarter millimetre the floating origin already allows itself.
//
// Sites (a town's levelled ground) ARE here now, and they are the one
// binding this file owns, because they have to be: the core's
// `Planet::surface` applies them, so a transcription that did not would be
// a function with the core's name and a different answer, and
// `ground_normal` has to see a plateau or a levelled town shades as the
// hill it replaced. Every material that imports this file declares binding
// 113 (`tiers::Tier` and `tiers::SeaTier` do), and the buffer is never
// empty: with no towns it carries one site whose skirt is behind the
// viewer, which weighs nought at every distance and needs no branch.
//
// A site's DIRECTION is carried as an OFFSET from the tier's anchor,
// differenced in f64 on the CPU like everything else here, because the
// weight is a function of how far a point is from the site's middle and
// that distance is metres on a planet of kilometres. Measured the obvious
// way, `acos(dot(dir, site))` in f32 near one resolves 3.5e-4 radians,
// which on a thousand kilometre planet is 346 m: a town is 80 m across, so
// the whole plateau would fall inside one step of the arithmetic. The
// difference of two offsets is a small number and the chord of an 80 m arc
// is the arc to eight significant figures.

// A planet's relief, passed by value: the tiers that use it own the
// uniform it comes out of, and the sites below are the one exception.
struct Planet {
    // Mean radius and the sea's, metres.
    radius: f32,
    sea: f32,
    // Peak to trough of the relief, metres, and how many of its features
    // fit round the planet.
    relief: f32,
    lumps: f32,
    // Octaves of relief, each halving the feature size, and the seed.
    octaves: u32,
    seed: u32,
}

// The towns' levelled ground, two lanes a site, which is
// `freeport_core::town::Site` with its direction taken to the anchor and
// its skirt worked out: lane 2i is that offset with the site's height over
// the mean radius in w, and lane 2i+1 is the arc inside which the ground
// is level and the arc past which it is the relief again, metres, which
// the CPU makes out of `r * 0.5` and the core's own SKIRT_IN and
// SKIRT_OUT so neither constant is written twice.
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var<storage, read> sites: array<vec4<f32>>;

// `field::mix3`: multiply, exclusive or and shift, and nothing else, so it
// is the same number here as in the core.
fn mix3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    var h: u32 = (bitcast<u32>(x) * 0x8DA6B343u)
        ^ (bitcast<u32>(y) * 0xD8163841u)
        ^ (bitcast<u32>(z) * 0xCB1AB31Fu)
        ^ (seed * 0x9E3779B9u);
    h ^= h >> 15u;
    h = h * 0x2C1B3C6Du;
    h ^= h >> 12u;
    h = h * 0x297A2D39u;
    h ^= h >> 15u;
    return h;
}

// `field::hash3_f32`: the mixing over its own range.
fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    return f32(mix3(x, y, z, seed)) / 4294967296.0;
}

// `field::smooth`, the smoothstep the lattice is faded across so it does
// not show as diamonds.
fn smooth3(t: f32) -> f32 {
    return t * t * (3.0 - 2.0 * t);
}

// `field::noise3`: value noise on the integer lattice, in 0..1.
fn noise3(p: vec3<f32>, seed: u32) -> f32 {
    let f = floor(p);
    let x = i32(f.x);
    let y = i32(f.y);
    let z = i32(f.z);
    let t = p - f;
    let tx = smooth3(t.x);
    let ty = smooth3(t.y);
    let tz = smooth3(t.z);
    let x00 = mix(hash3(x, y, z, seed), hash3(x + 1, y, z, seed), tx);
    let x10 = mix(hash3(x, y + 1, z, seed), hash3(x + 1, y + 1, z, seed), tx);
    let x01 = mix(hash3(x, y, z + 1, seed), hash3(x + 1, y, z + 1, seed), tx);
    let x11 = mix(hash3(x, y + 1, z + 1, seed), hash3(x + 1, y + 1, z + 1, seed), tx);
    return mix(mix(x00, x10, ty), mix(x01, x11, ty), tz);
}

// `field::fbm3`: each octave doubles the frequency and halves the weight,
// the sum divided by a norm that starts at one, exactly as the core's does.
fn fbm3(p: vec3<f32>, seed: u32, octaves: u32) -> f32 {
    var total = 0.0;
    var amp = 1.0;
    var norm = 0.0;
    var freq = 1.0;
    let n = max(octaves, 1u);
    for (var i = 0u; i < n; i = i + 1u) {
        total = total + amp * noise3(p * freq, seed + i);
        norm = norm + amp;
        amp = amp * 0.5;
        freq = freq * 2.0;
    }
    return total / norm;
}

// A planet off the two uniform lanes the tiers carry it in, so nothing
// outside this file spells the order of its fields.
fn planet_of(shape: vec4<f32>, counts: vec4<u32>) -> Planet {
    var p: Planet;
    p.radius = shape.x;
    p.sea = shape.y;
    p.relief = shape.z;
    p.lumps = shape.w;
    p.octaves = counts.x;
    p.seed = counts.y;
    return p;
}

// `field::Planet::site_weight`: how much a site levels a point, one right
// across it and nought past its skirt. `off` is the point's own offset
// from the anchor and `a`, `b` are the site's two lanes.
fn site_weight(a: vec4<f32>, b: vec4<f32>, radius: f32, off: vec3<f32>) -> f32 {
    let dist = length(off - a.xyz) * radius;
    return 1.0 - smoothstep(b.x, b.y, dist);
}

// `field::Planet::surface`: the relief at a direction, metres over the
// mean radius, with the towns' sites applied. `off` is the direction's own
// offset from the anchor, which is what the sites are measured against.
//
// There is deliberately no `ground` beside it, which would be this plus
// the radius. A caller on a thousand kilometre planet that forms one is a
// caller holding a number whose f32 step is 6 cm, and every one this file
// had was a bug: the normal differenced two of them (`ground_normal` says
// what that cost), the sea measured its own column off one, and a vertex
// was placed from one instead of from an offset. What a caller wants is
// the relief, added to a base the CPU worked out in f64.
fn surface(planet: Planet, dir: vec3<f32>, off: vec3<f32>) -> f32 {
    // A point right inside a site is the site's height and the noise is
    // never asked, which is the core's own early return: on a levelled
    // town that is a few dozen fbm octaves a vertex not evaluated.
    let n = arrayLength(&sites) / 2u;
    for (var i = 0u; i < n; i = i + 1u) {
        let a = sites[i * 2u];
        let b = sites[i * 2u + 1u];
        if (site_weight(a, b, planet.radius, off) >= 1.0) {
            return a.w;
        }
    }
    var s = (fbm3(dir * planet.lumps, planet.seed, planet.octaves) * 2.0 - 1.0)
        * planet.relief * 0.5;
    for (var i = 0u; i < n; i = i + 1u) {
        let a = sites[i * 2u];
        let w = site_weight(a, sites[i * 2u + 1u], planet.radius, off);
        if (w > 0.0) {
            s = s + (a.w - s) * w;
        }
    }
    return s;
}

// East and north at a direction, for stepping off it. The pole is swapped
// near the axis so the cross never collapses.
fn frame(up: vec3<f32>) -> mat2x3<f32> {
    var pole = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(up.y) >= 0.9) {
        pole = vec3<f32>(1.0, 0.0, 0.0);
    }
    let east = normalize(cross(pole, up));
    return mat2x3<f32>(east, normalize(cross(up, east)));
}

// The ground's outward normal at a direction, by central differences over
// `step` metres of arc. A triangle wide enough to shade smoothly asks for
// this; a hex column's top is flat and does not.
//
// The differences are of `surface` and NEVER of `ground`, which is the
// same number with the radius added: at a thousand kilometres an f32 step
// is 6 cm, so two radii differenced carry up to 12 cm of quantisation, and
// the nearest leaves the far tier draws are cut into sub triangles under a
// metre across, where the height actually gained over a step is a few
// centimetres. This is tenebris's own hex shader warning, that a gradient
// "must exceed planet scale f32 quantisation or the derivative degrades to
// noise", answered by never forming the planet scale number: the radius
// cancels out of a difference exactly, so it is left out rather than added
// and taken away again. The picture barely moved when this changed (0.000%
// of pixels over 8 of 255 on the grazing shot, worst 1), because the band
// where a sub triangle is that small is a few metres wide just past the
// hex disc and everything beyond it steps in metres. It is still the
// difference between a normal that is right by construction and one that
// is right because the triangles happen to be big.
fn ground_normal(planet: Planet, dir: vec3<f32>, off: vec3<f32>, step: f32) -> vec3<f32> {
    let f = frame(dir);
    let d = step / planet.radius;
    let east = f[0];
    let north = f[1];
    // The stepped OFFSET is the offset plus the step: normalising would
    // take a second order correction off it, `step * step / (2 * radius)`,
    // which at a metre on this planet is half a micron, and the sites are
    // measured in metres.
    let he = surface(planet, normalize(dir + east * d), off + east * d)
        - surface(planet, normalize(dir - east * d), off - east * d);
    let hn = surface(planet, normalize(dir + north * d), off + north * d)
        - surface(planet, normalize(dir - north * d), off - north * d);
    // The surface is r(dir) along dir, so its tangents are the arc step
    // along each axis plus the height it gained over that step.
    let te = east * (2.0 * step) + dir * he;
    let tn = north * (2.0 * step) + dir * hn;
    let n = normalize(cross(te, tn));
    if (dot(n, dir) < 0.0) {
        return -n;
    }
    return n;
}
