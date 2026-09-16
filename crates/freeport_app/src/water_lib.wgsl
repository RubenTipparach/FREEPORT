#define_import_path freeport::water

// The sheet's own arithmetic, out of `water.wgsl` so the two things that
// lift a sea surface share one: the dual contoured chunks' sheet, whose
// vertices are a mesh, and the hex world's, whose vertices are made in
// `tiers.wgsl` off nothing. A swell written twice is a sheet that would
// meet itself at a step wherever the two tiers touch.
//
// It is tenebris's `water.fs.glsl` and `water.vs.glsl`: the gradient
// noise, the three octave `fbm3` drifting three ways, and the swell along
// the radial.

fn gn_fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn gn_hash(x: i32, y: i32, z: i32) -> u32 {
    let h = (u32(x + 1073741824) * 374761393u) ^ (u32(y + 1073741824) * 668265263u) ^ (u32(z + 1073741824) * 1274126177u);
    let g = (h ^ (h >> 13u)) * 1103515245u;
    return g ^ (g >> 16u);
}

fn gn_grad(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15u;
    let u = select(y, x, h < 8u);
    var v: f32;
    if (h < 4u) {
        v = y;
    } else {
        v = select(z, x, h == 12u || h == 14u);
    }
    let a = select(u, -u, (h & 1u) != 0u);
    let b = select(v, -v, (h & 2u) != 0u);
    return a + b;
}

// Gradient noise, tenebris's gnoise3.
fn gnoise3(p: vec3<f32>) -> f32 {
    let f = floor(p);
    let xi = i32(f.x);
    let yi = i32(f.y);
    let zi = i32(f.z);
    let d = p - f;
    let u = gn_fade(d.x);
    let v = gn_fade(d.y);
    let w = gn_fade(d.z);
    let n000 = gn_grad(gn_hash(xi, yi, zi), d.x, d.y, d.z);
    let n100 = gn_grad(gn_hash(xi + 1, yi, zi), d.x - 1.0, d.y, d.z);
    let n010 = gn_grad(gn_hash(xi, yi + 1, zi), d.x, d.y - 1.0, d.z);
    let n110 = gn_grad(gn_hash(xi + 1, yi + 1, zi), d.x - 1.0, d.y - 1.0, d.z);
    let n001 = gn_grad(gn_hash(xi, yi, zi + 1), d.x, d.y, d.z - 1.0);
    let n101 = gn_grad(gn_hash(xi + 1, yi, zi + 1), d.x - 1.0, d.y, d.z - 1.0);
    let n011 = gn_grad(gn_hash(xi, yi + 1, zi + 1), d.x, d.y - 1.0, d.z - 1.0);
    let n111 = gn_grad(gn_hash(xi + 1, yi + 1, zi + 1), d.x - 1.0, d.y - 1.0, d.z - 1.0);
    return mix(
        mix(mix(n000, n100, u), mix(n010, n110, u), v),
        mix(mix(n001, n101, u), mix(n011, n111, u), v),
        w,
    );
}

// Three octaves drifting three ways, tenebris's fbm3.
fn fbm3(p: vec3<f32>, t: f32) -> f32 {
    var q = p * 0.9 + vec3(t * 0.35, t * 0.18, t * -0.42);
    var h = gnoise3(q) * 0.5;
    q = q * 2.1 + vec3(t * -0.22, t * 0.33, t * 0.17);
    h += gnoise3(q) * 0.28;
    q = q * 2.3 + vec3(t * 0.19, t * -0.27, t * 0.11);
    h += gnoise3(q) * 0.15;
    return h;
}


// The swell along the radial, tenebris's water.vs.glsl: three sines
// crossing at different rates so the sheet never reads as one wave.
// `q` is planet local, `wave` and `freq` the material's own lanes.
fn swell(q: vec3<f32>, wave: vec4<f32>, freq: f32, time: f32) -> f32 {
    let t = time * wave.x;
    return (sin(t * 0.9 + q.x * 1.4 * freq + q.z * 0.6 * freq) * 0.18
        + sin(t * 1.3 - q.x * 0.7 * freq + q.z * 1.2 * freq) * 0.12
        + sin(t * 1.7 + q.x * 2.3 * freq - q.z * 1.9 * freq) * 0.06) * wave.w;
}
