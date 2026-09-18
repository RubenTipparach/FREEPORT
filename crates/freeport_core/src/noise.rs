//! The arithmetic under the ground: the integer hash, the value noise it
//! makes, the fractal sum of that, and the grid a chunk's samples land in.
//!
//! It is its own module because it is its own thing: `field` answers what
//! is AT a point and this answers what a number at a point is made of, and
//! the second has no idea the first exists. `field` re-exports every name
//! here, so nothing outside the crate learned a new path when it moved,
//! which is what `damage`, `heat` and `wound` did in swarm-demo for the
//! same reason.
//!
//! Nothing here uses `sin`, and that is the whole rule: a field two
//! clients evaluate has to come out bit for bit the same on both, and a
//! transcendental does not. Add, multiply, floor and compare.

use crate::field::Density;
use glam::DVec3;

/// The lattice's mixing, in whole numbers: nothing but multiply, exclusive
/// or and shift, so it is bit exact on every machine and in every language.
/// `hash3` is this over its own range and `field.wgsl` computes exactly
/// this in WGSL.
pub fn mix3(x: i64, y: i64, z: i64, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

/// A lattice hash in 0..1, bit exact on every machine: what the noise is
/// built on, and what a plan draws its dice from.
pub fn hash3(x: i64, y: i64, z: i64, seed: u32) -> f64 {
    mix3(x, y, z, seed) as f64 / 4_294_967_296.0
}

/// The same hash as a float, which is all a GPU can hold: `f32` carries
/// twenty four bits of a thirty two bit number, so this is where the
/// transcription in `field.wgsl` parts company with the core, and
/// `a_float_hash_is_the_cores_to_a_hundred_millionth` is the bound.
pub fn hash3_f32(x: i64, y: i64, z: i64, seed: u32) -> f32 {
    mix3(x, y, z, seed) as f32 / 4_294_967_296.0
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on the integer lattice, in 0..1, smoothstepped so the lattice
/// does not show as diamonds.
pub fn noise3(p: DVec3, seed: u32) -> f64 {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // Detection guards every use; other architectures retain the scalar
        // reference and no executable-wide CPU feature flag is required.
        return unsafe { simd::noise3(p, seed) };
    }
    noise3_scalar(p, seed)
}

fn noise3_scalar(p: DVec3, seed: u32) -> f64 {
    let f = p.floor();
    let (x, y, z) = (f.x as i64, f.y as i64, f.z as i64);
    let t = p - f;
    let (tx, ty, tz) = (smooth(t.x), smooth(t.y), smooth(t.z));
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let c = |dx: i64, dy: i64, dz: i64| hash3(x + dx, y + dy, z + dz, seed);
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), tx);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), tx);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), tx);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

#[cfg(target_arch = "x86_64")]
mod simd;

/// Fractal sum of `noise3`, in 0..1: each octave doubles the frequency and
/// halves the weight.
pub fn fbm3(p: DVec3, seed: u32, octaves: u32) -> f64 {
    let (mut total, mut amp, mut norm, mut freq) = (0.0, 1.0, 0.0, 1.0);
    for i in 0..octaves.max(1) {
        total += amp * noise3(p * freq, seed.wrapping_add(i));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    total / norm
}

/// A chunk of a field sampled on a lattice: `n` cells a side, `cell` metres
/// each, starting at `corner`, with one cell of apron all round so a normal
/// can be taken by central differences at every lattice point of the chunk.
pub struct Grid {
    pub n: usize,
    pub cell: f64,
    pub corner: DVec3,
    values: Vec<f32>,
}

impl Grid {
    fn stride(&self) -> usize {
        self.n + 3
    }

    fn index(&self, i: i32, j: i32, k: i32) -> usize {
        let s = self.stride();
        ((k + 1) as usize * s + (j + 1) as usize) * s + (i + 1) as usize
    }

    /// The density at lattice point `i, j, k`, each in -1..=n+1.
    pub fn at(&self, i: i32, j: i32, k: i32) -> f32 {
        self.values[self.index(i, j, k)]
    }

    /// The field's gradient at a lattice point in 0..=n by central
    /// differences, in density per cell.
    pub fn gradient(&self, i: i32, j: i32, k: i32) -> [f32; 3] {
        [
            (self.at(i + 1, j, k) - self.at(i - 1, j, k)) * 0.5,
            (self.at(i, j + 1, k) - self.at(i, j - 1, k)) * 0.5,
            (self.at(i, j, k + 1) - self.at(i, j, k - 1)) * 0.5,
        ]
    }

    /// The world point of a lattice point, in the field's frame.
    pub fn point(&self, i: i32, j: i32, k: i32) -> DVec3 {
        self.corner + DVec3::new(i as f64, j as f64, k as f64) * self.cell
    }
}

/// Sample `field` over a chunk. The density is evaluated in `f64` and stored
/// as `f32` because a chunk is a few tens of metres across and the number
/// that matters, the density's distance from nought, is small there whatever
/// the planet's radius is.
pub fn sample(field: &dyn Density, corner: DVec3, cell: f64, n: usize) -> Grid {
    let s = n + 3;
    let mut values = Vec::with_capacity(s * s * s);
    for k in -1..=(n as i32 + 1) {
        for j in -1..=(n as i32 + 1) {
            for i in -1..=(n as i32 + 1) {
                let p = corner + DVec3::new(i as f64, j as f64, k as f64) * cell;
                values.push(field.at(p) as f32);
            }
        }
    }
    Grid {
        n,
        cell,
        corner,
        values,
    }
}
