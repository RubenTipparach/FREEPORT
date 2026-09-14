//! What the ground is made of at a point: density fields, positive inside.
//!
//! A planet's terrain is a FIELD rather than a height map or a column of
//! blocks: a function from a point in the planet's frame to a density that is
//! positive in rock and negative in air, and the surface is where it crosses
//! nought. That is what lets the ground overhang, cave and arch, which a
//! height map cannot and a column of hex prisms can only fake, and it is
//! what the mesher (`march`) turns into triangles. Everything here is `f64`
//! and built out of add, multiply, floor and compare, because a field two
//! clients evaluate has to come out bit for bit the same on both, and
//! `sin` does not (see `tools/texkit.py` for the same rule on textures).

use glam::DVec3;

/// A density: positive inside the rock, negative in the air, nought on the
/// surface.
pub trait Density {
    /// The density at `p`, in the field's own frame, in metres.
    fn at(&self, p: DVec3) -> f64;
}

/// A ball of rock and nothing else.
pub struct Sphere {
    pub radius: f64,
}

impl Density for Sphere {
    fn at(&self, p: DVec3) -> f64 {
        self.radius - p.length()
    }
}

/// A planet: a sphere with fractal relief on its surface and a little
/// three dimensional noise so a slope can overhang.
pub struct Planet {
    /// Mean radius, metres.
    pub radius: f64,
    /// Peak to trough of the surface relief, metres.
    pub relief: f64,
    /// How many relief features fit round the planet: the base frequency of
    /// the surface noise in cycles per unit direction.
    pub lumps: f64,
    /// Octaves of surface relief. Each halves the feature size.
    pub octaves: u32,
    /// Amplitude of the volumetric term, metres of density, which is what
    /// lets cliffs undercut. Nought is a pure height field.
    pub overhang: f64,
    /// Feature size of the volumetric term, metres.
    pub ledge: f64,
    pub seed: u32,
}

impl Default for Planet {
    fn default() -> Self {
        Planet {
            radius: 1.0e6,
            relief: 8_000.0,
            lumps: 12.0,
            octaves: 8,
            overhang: 20.0,
            ledge: 60.0,
            seed: 7,
        }
    }
}

impl Density for Planet {
    fn at(&self, p: DVec3) -> f64 {
        let r = p.length();
        if r == 0.0 {
            return self.radius;
        }
        let dir = p / r;
        let surface =
            (fbm3(dir * self.lumps, self.seed, self.octaves) * 2.0 - 1.0) * self.relief * 0.5;
        let carve = if self.overhang > 0.0 && self.ledge > 0.0 {
            (noise3(p / self.ledge, self.seed.wrapping_add(0x9E37)) - 0.5) * self.overhang
        } else {
            0.0
        };
        self.radius + surface - r + carve
    }
}

/// A box in a frame of its own: a floor slab, a wall, a step. Positive
/// inside, and a signed distance outside its faces, so a crossing bisected
/// on it lands on the face and a vertex solved from its crossings' planes
/// lands on the corner.
#[derive(Clone, Debug)]
pub struct Block {
    /// The box's middle.
    pub centre: DVec3,
    /// Half its extent along each of its own axes, metres.
    pub half: DVec3,
    /// Its axes, unit and orthogonal: east, north, up.
    pub axes: [DVec3; 3],
}

impl Density for Block {
    fn at(&self, p: DVec3) -> f64 {
        let d = p - self.centre;
        let q = DVec3::new(
            d.dot(self.axes[0]).abs() - self.half.x,
            d.dot(self.axes[1]).abs() - self.half.y,
            d.dot(self.axes[2]).abs() - self.half.z,
        );
        let outside = q.max(DVec3::ZERO).length();
        let inside = q.x.max(q.y).max(q.z).min(0.0);
        -(outside + inside)
    }
}

/// A field with things built on it: the ground, and blocks ADDED to it in
/// order, each a union. What is built is what the fine lattice is for, and
/// the coarse one never sees it: the mask covers every cell a block touches.
pub struct Built<'a> {
    pub ground: &'a dyn Density,
    pub blocks: Vec<Block>,
}

impl Density for Built<'_> {
    fn at(&self, p: DVec3) -> f64 {
        let mut d = self.ground.at(p);
        for b in &self.blocks {
            d = d.max(b.at(p));
        }
        d
    }
}

impl Block {
    /// The corners of the box, in the field's frame.
    pub fn corners(&self) -> [DVec3; 8] {
        std::array::from_fn(|c| {
            let sx = if c & 1 != 0 { 1.0 } else { -1.0 };
            let sy = if c & 2 != 0 { 1.0 } else { -1.0 };
            let sz = if c & 4 != 0 { 1.0 } else { -1.0 };
            self.centre
                + self.axes[0] * (self.half.x * sx)
                + self.axes[1] * (self.half.y * sy)
                + self.axes[2] * (self.half.z * sz)
        })
    }
}

/// A lattice hash in 0..1, bit exact on every machine.
fn hash3(x: i64, y: i64, z: i64, seed: u32) -> f64 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h as f64 / 4_294_967_296.0
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on the integer lattice, in 0..1, smoothstepped so the lattice
/// does not show as diamonds.
pub fn noise3(p: DVec3, seed: u32) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_in_range_and_deterministic() {
        for i in 0..200 {
            let p = DVec3::new(i as f64 * 0.37, i as f64 * -0.11, 3.0 + i as f64 * 0.05);
            let a = noise3(p, 5);
            assert!((0.0..=1.0).contains(&a));
            assert_eq!(a, noise3(p, 5));
            assert!((0.0..=1.0).contains(&fbm3(p, 5, 6)));
        }
        assert_ne!(
            noise3(DVec3::new(0.5, 0.5, 0.5), 1),
            noise3(DVec3::new(0.5, 0.5, 0.5), 2)
        );
    }

    #[test]
    fn noise_is_continuous_across_a_lattice_line() {
        let a = noise3(DVec3::new(2.0 - 1e-9, 0.3, 0.7), 9);
        let b = noise3(DVec3::new(2.0 + 1e-9, 0.3, 0.7), 9);
        assert!((a - b).abs() < 1e-6);
    }

    #[test]
    fn a_planet_is_rock_inside_and_air_outside() {
        let planet = Planet::default();
        let inside = DVec3::new(0.5 * planet.radius, 0.0, 0.0);
        let outside = DVec3::new(0.0, planet.radius + planet.relief, 0.0);
        assert!(planet.at(inside) > 0.0);
        assert!(planet.at(outside) < 0.0);
        assert!(planet.at(DVec3::ZERO) > 0.0);
        let surface = planet.at(DVec3::new(0.0, 0.0, planet.radius));
        assert!(surface.abs() <= planet.relief * 0.5 + planet.overhang);
    }

    #[test]
    fn a_block_is_a_signed_distance_in_its_own_frame() {
        let b = Block {
            centre: DVec3::new(1.0, 2.0, 3.0),
            half: DVec3::new(2.0, 1.0, 0.5),
            axes: [DVec3::Z, DVec3::X, DVec3::Y],
        };
        assert_eq!(b.at(b.centre), 0.5);
        // A metre past the up face (world y) is minus one.
        assert!((b.at(b.centre + DVec3::Y * 1.5) + 1.0).abs() < 1e-12);
        // Along the box's east (world z) the half extent is two.
        assert!((b.at(b.centre + DVec3::Z * 2.0)).abs() < 1e-12);
        assert!((b.at(b.centre + DVec3::new(0.0, 1.5, 3.0)) + 2.0f64.sqrt()).abs() < 1e-12);
        let corners = b.corners();
        assert!(corners.iter().all(|c| b.at(*c).abs() < 1e-12));
        let ground = Sphere { radius: 1.0 };
        let built = Built {
            ground: &ground,
            blocks: vec![b.clone()],
        };
        assert_eq!(built.at(b.centre), 0.5);
        assert_eq!(built.at(DVec3::ZERO), 1.0);
    }

    #[test]
    fn a_grid_carries_its_apron_and_a_gradient() {
        let grid = sample(
            &Sphere { radius: 5.0 },
            DVec3::new(-8.0, -8.0, -8.0),
            1.0,
            16,
        );
        assert_eq!(grid.at(-1, -1, -1), (5.0 - (3.0f64 * 81.0).sqrt()) as f32);
        assert_eq!(grid.at(8, 8, 8), 5.0);
        let g = grid.gradient(12, 8, 8);
        assert!(
            g[0] < 0.0 && g[1].abs() < 1e-6 && g[2].abs() < 1e-6,
            "{g:?}"
        );
        assert_eq!(grid.point(0, 0, 0), grid.corner);
    }
}
