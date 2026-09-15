#define_import_path freeport::atmos

// The air, on the GPU: `freeport_core::atmos` transcribed, which is
// tenebris's `atmosphere.fs.glsl` and a GPU Gems 2 single scatter march
// under that. Eight samples along the view ray and four out to the sun
// from each, the same phase functions and the same dusk terms, so the
// dome this draws and the fog the core hands the ground are one march
// asked twice rather than two things tuned to look alike.
//
// Every position is PLANET LOCAL metres. The one deliberate difference
// from the core: this dithers the march by a hash of the pixel, because a
// fixed offset bands a gradient across a screen, and the core takes the
// middle of each step because it has no pixel and wants the same answer
// twice.

struct Air {
    // Where the march measures its density from, where a view ray is
    // STOPPED going down, and where it stops going up. `atmos::Air`'s
    // own `floor` says why the second is not the first.
    ground: f32,
    floor: f32,
    top: f32,
    // The density falloff, in normalised altitude.
    scale_height: f32,
    rayleigh: f32,
    mie: f32,
    mie_g: f32,
    // The sky's brightness, and the 1/lambda^4 ratios that are its hue.
    sun: f32,
    waves: vec3<f32>,
    // The dusk terms: how much, the tint, the sun's glow, the horizon.
    sunset: f32,
    tint: vec3<f32>,
    glow: vec3<f32>,
    band: vec3<f32>,
}

// The lanes a material carries an `Air` in, so nothing outside this file
// spells the order of its fields.
fn air_of(
    shell: vec4<f32>,
    coef: vec4<f32>,
    waves: vec4<f32>,
    tint: vec4<f32>,
    glow: vec4<f32>,
    band: vec4<f32>,
) -> Air {
    var a: Air;
    a.ground = shell.x;
    a.top = shell.y;
    a.floor = shell.z;
    a.scale_height = coef.x;
    a.rayleigh = coef.y;
    a.mie = coef.z;
    a.mie_g = coef.w;
    a.sun = waves.w;
    a.waves = waves.xyz;
    a.sunset = tint.w;
    a.tint = tint.xyz;
    a.glow = glow.xyz;
    a.band = band.xyz;
    return a;
}

fn thickness(air: Air) -> f32 {
    return max(air.top - air.ground, 1.0e-6);
}

// `atmos::ray_sphere`: the two roots, or a pair of minus ones on a miss.
fn ray_sphere(start: vec3<f32>, dir: vec3<f32>, radius: f32) -> vec2<f32> {
    let b = dot(start, dir);
    let c = dot(start, start) - radius * radius;
    let disc = b * b - c;
    if (disc < 0.0) {
        return vec2<f32>(-1.0, -1.0);
    }
    let s = sqrt(disc);
    return vec2<f32>(-b - s, -b + s);
}

fn density(air: Air, alt: f32) -> f32 {
    return exp(-alt / air.scale_height);
}

// `atmos::optical_depth`, four samples out toward the sun.
fn optical_depth(air: Air, start: vec3<f32>, dir: vec3<f32>, len: f32) -> f32 {
    let thick = thickness(air);
    let step = len * 0.25;
    let n = step / thick;
    var depth = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        let at = start + dir * (step * (f32(i) + 0.5));
        let alt = clamp((length(at) - air.ground) / thick, 0.0, 1.0);
        depth = depth + density(air, alt) * n;
    }
    return depth;
}

fn rayleigh_phase(c: f32) -> f32 {
    return 0.75 * (1.0 + c * c);
}

fn mie_phase(c: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (12.5663706 * pow(max((1.0 + g2) - (2.0 * g * c), 1.0e-4), 1.5));
}

// A dither so eight samples do not band across a gradient.
fn jitter(at: vec2<f32>) -> f32 {
    return fract(sin(dot(at, vec2<f32>(127.1, 311.7))) * 43758.5453) * 0.5;
}

// The two sums a march comes back with.
struct Sums {
    rayleigh: vec3<f32>,
    mie: vec3<f32>,
}

// `atmos::gather`: what a view ray picks up between `near` and `far`.
// Both sums are vec3, and the Mie one is NOT grey: its samples are
// attenuated by the same per wavelength extinction the Rayleigh ones
// are, so collapsing it to one channel tints the haze and washes the sky
// out. The first cut did exactly that and drew a white sky under a
// correct ground.
fn gather(
    air: Air,
    eye: vec3<f32>,
    dir: vec3<f32>,
    near: f32,
    far: f32,
    sun: vec3<f32>,
    dither: f32,
) -> Sums {
    let thick = thickness(air);
    let step = (far - near) * 0.125;
    let n = step / thick;
    var rayleigh = vec3<f32>(0.0);
    var mie = vec3<f32>(0.0);
    var depth = 0.0;
    for (var i = 0; i < 8; i = i + 1) {
        let at = eye + dir * (near + step * (f32(i) + dither));
        let h = length(at);
        // `air.floor` and never `air.ground`: the core's `gather` says
        // what skipping on the mean radius did to a sky looked at from
        // under it.
        if (h < air.floor) {
            continue;
        }
        let alt = clamp((h - air.ground) / thick, 0.0, 1.0);
        let here = density(air, alt);
        depth = depth + here * n;
        if (ray_sphere(at, sun, air.ground).x > 0.0) {
            continue;
        }
        let out = max(ray_sphere(at, sun, air.top).y, 0.0);
        let to_sun = optical_depth(air, at, sun, out);
        let att = exp(-(air.waves * air.rayleigh * (depth + to_sun)
            + vec3<f32>(air.mie * (depth + to_sun))));
        let contrib = att * here * n;
        rayleigh = rayleigh + contrib;
        mie = mie + contrib;
    }
    var out: Sums;
    out.rayleigh = rayleigh * air.waves * air.rayleigh;
    out.mie = mie * air.mie;
    return out;
}

fn smooth_between(a: f32, b: f32, t: f32) -> f32 {
    let k = clamp((t - a) / (b - a), 0.0, 1.0);
    return k * k * (3.0 - 2.0 * k);
}

// `atmos::dusk`.
fn dusk(height: f32) -> f32 {
    return smooth_between(0.18, 0.0, height) * smooth_between(-0.32, -0.1, height);
}

// `atmos::sky`: the colour along a view ray in `xyz` and how much of it
// there is in `w`, one where the ray meets the ground and the scatter's
// own brightness where it leaves for space.
fn sky(air: Air, eye: vec3<f32>, look: vec3<f32>, sun_at: vec3<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let dir = normalize(look);
    let sun = normalize(sun_at);
    let shell = ray_sphere(eye, dir, air.top);
    if (shell.y < 0.0) {
        return vec4<f32>(0.0);
    }
    let near = max(shell.x, 0.0);
    // `air.floor` and never `air.ground`: the core's field says why, and
    // the conditioning is the other half of it, since in `f32` the
    // difference of two squares at 10^12 is quantised to 131 km^2.
    let ground = ray_sphere(eye, dir, air.floor).x;
    let hits = ground > 0.0;
    var far = shell.y;
    if (hits) {
        far = min(far, ground);
    }
    if (near >= far) {
        return vec4<f32>(0.0);
    }
    let sums = gather(air, eye, dir, near, far, sun, jitter(pixel));
    let c = dot(dir, sun);
    let up = normalize(eye);
    let at_dusk = dusk(dot(sun, up));
    var scattered = (sums.rayleigh * rayleigh_phase(c)
        + sums.mie * mie_phase(c, air.mie_g)) * air.sun;
    scattered = scattered * mix(vec3<f32>(1.0), air.tint, at_dusk * air.sunset);
    let glow = air.glow * (pow(max(c, 0.0), 3.0) * at_dusk) * air.sun * 0.22;
    let band = air.band * (pow(1.0 - abs(dot(dir, up)), 3.0) * at_dusk) * air.sun * 0.11;
    // The radiance as it is: `atmos::sky` says why there is no
    // `1 - exp` here where tenebris's GLSL has one.
    let colour = scattered + glow + band;
    var alpha = 1.0;
    if (!hits) {
        alpha = clamp(1.0 - exp(-length(colour) * 1.5), 0.0, 0.95);
    }
    return vec4<f32>(colour, alpha);
}
