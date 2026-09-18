// The noise functions transcribe freeport_core::field::{mix3, noise3, fbm3}.
// The CPU subtracts radii and blends town sites in f64. Coordinates arrive
// as high/low pairs: adding them before floor would lose fine noise at the
// highest octaves on a thousand kilometre planet.
struct Point {
    relief_hi: vec4<f32>, // xyz noise coordinate, w radial density + site bias
    relief_lo: vec4<f32>, // xyz low part, w relief amplitude after site blend
    carve_hi: vec4<f32>,  // xyz noise coordinate, w carve amplitude
    carve_lo: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> points: array<Point>;
@group(0) @binding(1) var<storage, read_write> densities: array<f32>;
@group(0) @binding(2) var<uniform> settings: vec4<u32>; // seed, octaves, count, error bits

fn hash(p: vec3<i32>, seed: u32) -> f32 {
    var h = u32(p.x) * 0x8DA6B343u ^ u32(p.y) * 0xD8163841u
        ^ u32(p.z) * 0xCB1AB31Fu ^ seed * 0x9E3779B9u;
    h ^= h >> 15u;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12u;
    h *= 0x297A2D39u;
    h ^= h >> 15u;
    return f32(h) / 4294967296.0;
}

fn noise(hi: vec3<f32>, lo: vec3<f32>, seed: u32) -> f32 {
    let whole = floor(hi);
    let tail = hi - whole + lo;
    let carry = floor(tail);
    let cell = vec3<i32>(whole) + vec3<i32>(carry);
    let t = tail - carry;
    let s = t * t * (3.0 - 2.0 * t);
    let x00 = mix(hash(cell, seed), hash(cell + vec3<i32>(1, 0, 0), seed), s.x);
    let x10 = mix(hash(cell + vec3<i32>(0, 1, 0), seed), hash(cell + vec3<i32>(1, 1, 0), seed), s.x);
    let x01 = mix(hash(cell + vec3<i32>(0, 0, 1), seed), hash(cell + vec3<i32>(1, 0, 1), seed), s.x);
    let x11 = mix(hash(cell + vec3<i32>(0, 1, 1), seed), hash(cell + vec3<i32>(1, 1, 1), seed), s.x);
    return mix(mix(x00, x10, s.y), mix(x01, x11, s.y), s.z);
}


// ---------------------------------------------------------------------
// The relief's own terms, a transcription of
// `freeport_core::biome::Shape::landform` and `::cut`. The SHAPE of the
// function is here and every constant in it arrives in `shape`, so a
// threshold cannot be tuned on one side and left stale on the other: a
// planet whose mesher and whose walker disagreed about where a mountain
// belt starts is ground you fall through.
//
// Everything is a function of the DIRECTION, which arrives as the same
// high and low pair the octaves already use, so multiplying by a term's
// own frequency costs no precision at a thousand kilometres.

struct Shape {
    shares: vec4<f32>,   // continent, mountain, hills, valley
    freqs: vec4<f32>,    // continent, belt, ridge, hills
    channel: vec4<f32>,  // frequency, lip, gorge metres, reach metres
    belt: vec4<f32>,     // from, to, ridge power, stretch
    fbm: vec4<f32>,      // mean, span, half the relief, gain
    octaves: vec4<u32>,  // continent, belt, ridge, channel
    salts: vec4<u32>,    // the seeds those four are salted with
    hills: vec4<u32>,    // the hills term's salt and octaves
}
@group(0) @binding(3) var<uniform> shape: Shape;

// `field::fbm3`, with the coordinate kept split.
fn fbm(hi: vec3<f32>, lo: vec3<f32>, base: f32, seed: u32, octaves: u32) -> f32 {
    var total = 0.0;
    var amp = 1.0;
    var norm = 0.0;
    var frequency = base;
    for (var i = 0u; i < octaves; i++) {
        total += amp * noise(hi * frequency, lo * frequency, seed + i);
        norm += amp;
        amp *= 0.5;
        frequency *= 2.0;
    }
    return total / norm;
}

// `biome::signed`: the measured spread stretched to minus one through one.
fn signed(v: f32) -> f32 {
    return clamp((v - shape.fbm.x) / shape.fbm.y, -1.0, 1.0);
}

// `biome::ridged`.
fn ridged(n: f32) -> f32 {
    return pow(clamp(1.0 - abs(signed(n)), 0.0, 1.0), shape.belt.z);
}

// `biome::Shape::landform`, metres over the mean radius.
fn landform(hi: vec3<f32>, lo: vec3<f32>) -> f32 {
    let half = shape.fbm.z;
    let continent = signed(fbm(hi, lo, shape.freqs.x, settings.x + shape.salts.x, shape.octaves.x))
        * half * shape.shares.x;
    let belt = fbm(hi, lo, shape.freqs.y, settings.x + shape.salts.y, shape.octaves.y);
    let weight = smoothstep(shape.belt.x, shape.belt.y, belt);
    var mountain = 0.0;
    if weight > 0.0 {
        mountain = ridged(fbm(hi, lo, shape.freqs.z, settings.x + shape.salts.z, shape.octaves.z))
            * weight * half * shape.shares.y;
    }
    return continent + mountain;
}

// `biome::Shape::cut`, metres taken off the landform.
fn cut(hi: vec3<f32>, lo: vec3<f32>, standing: f32) -> f32 {
    if standing <= 0.0 {
        return 0.0;
    }
    let n = fbm(hi, lo, shape.channel.x, settings.x + shape.salts.w, shape.octaves.w);
    let c = smoothstep(shape.channel.y, 1.0, clamp(1.0 - abs(signed(n)), 0.0, 1.0));
    if c <= 0.0 {
        return 0.0;
    }
    let reach = smoothstep(0.0, shape.channel.w, standing);
    let broad = shape.fbm.z * shape.shares.w * c;
    return (broad + shape.channel.z * c) * reach;
}

@compute @workgroup_size(64)
fn sample(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let i = invocation.x;
    if i >= settings.z { return; }
    let p = points[i];
    var density = p.relief_hi.w;
    if p.carve_hi.w != 0.0 {
        density += (noise(p.carve_hi.xyz, p.carve_lo.xyz, settings.x + 0x9E37u) - 0.5)
            * p.carve_hi.w;
    }
    // How much of the relief this direction keeps: nought right across a
    // levelled town site, where the ground is the site's own height and
    // the noise is not asked at all.
    let keep = p.relief_lo.w;
    if keep != 0.0 {
        let hi = p.relief_hi.xyz;
        let lo = p.relief_lo.xyz;
        let gain = shape.fbm.w;
        // The landform and what the water cuts into it, in full: these
        // are the terms a planet is READ by, and they are few octaves
        // each, so there is nothing here worth stopping early for.
        let form = landform(hi, lo);
        density += (form - cut(hi, lo, form)) * gain * keep;
        // The hills, which are the ground under a walker's feet and carry
        // the body's whole octave count. Contouring uses only the SIGN of
        // this, so the remaining octaves need not be evaluated once they
        // cannot reach nought: an octave contributes between nought and
        // its own amplitude, and `signed` is monotone, so an interval on
        // the partial sum is an interval on the height.
        let amp = shape.fbm.z * shape.shares.z * gain * keep;
        let count = shape.hills.y;
        let seed = settings.x + shape.hills.x;
        let norm = 2.0 - exp2(1.0 - f32(count));
        let error = 2.0 * bitcast<f32>(settings.w);
        var total = 0.0;
        var octave_amp = 1.0;
        var remaining = norm;
        var frequency = shape.freqs.w;
        for (var octave = 0u; octave < count; octave++) {
            let low = density + signed(total / norm) * amp;
            let high = density + signed((total + remaining) / norm) * amp;
            let middle = 0.5 * (low + high);
            let reach = 0.5 * abs(high - low);
            if abs(middle) > reach + error {
                densities[i] = middle;
                return;
            }
            total += octave_amp * noise(hi * frequency, lo * frequency, seed + octave);
            remaining = max(0.0, remaining - octave_amp);
            octave_amp *= 0.5;
            frequency *= 2.0;
        }
        density += signed(total / norm) * amp;
    }
    densities[i] = density;
}
