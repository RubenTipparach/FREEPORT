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
// Sites (a town's levelled ground) are NOT here. They are a list, and a
// list is a binding rather than a value; the tiers that draw a town will
// take one and this file will grow a `site_weight` beside `surface`.

// A planet's relief, passed by value so this file owns no binding: the
// tiers that use it own the uniform it comes out of.
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

// `field::Planet::surface` with no sites: the relief at a direction, metres
// over the mean radius.
fn surface(planet: Planet, dir: vec3<f32>) -> f32 {
    let n = fbm3(dir * planet.lumps, planet.seed, planet.octaves);
    return (n * 2.0 - 1.0) * planet.relief * 0.5;
}

// Where the ground is along a direction, metres from the planet's centre.
fn ground(planet: Planet, dir: vec3<f32>) -> f32 {
    return planet.radius + surface(planet, dir);
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
fn ground_normal(planet: Planet, dir: vec3<f32>, step: f32) -> vec3<f32> {
    let f = frame(dir);
    let d = step / planet.radius;
    let east = f[0];
    let north = f[1];
    let he = ground(planet, normalize(dir + east * d)) - ground(planet, normalize(dir - east * d));
    let hn = ground(planet, normalize(dir + north * d)) - ground(planet, normalize(dir - north * d));
    let r = ground(planet, dir);
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

// Nought at the sea's surface, positive in the water: what says a column's
// top is a beach, a shallow or a sea floor.
fn depth(planet: Planet, p: vec3<f32>) -> f32 {
    return planet.sea - length(p);
}
