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
@group(0) @binding(2) var<uniform> settings: vec4<u32>; // seed, octaves, count

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

@compute @workgroup_size(64)
fn sample(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let i = invocation.x;
    if i >= settings.z { return; }
    let p = points[i];
    var density = p.relief_hi.w;
    if p.relief_lo.w != 0.0 {
        var total = 0.0;
        var amplitude = 1.0;
        var norm = 0.0;
        var frequency = 1.0;
        for (var octave = 0u; octave < settings.y; octave++) {
            total += amplitude * noise(p.relief_hi.xyz * frequency,
                p.relief_lo.xyz * frequency, settings.x + octave);
            norm += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        density += (2.0 * total / norm - 1.0) * p.relief_lo.w;
    }
    if p.carve_hi.w != 0.0 {
        density += (noise(p.carve_hi.xyz, p.carve_lo.xyz, settings.x + 0x9E37u) - 0.5)
            * p.carve_hi.w;
    }
    densities[i] = density;
}
