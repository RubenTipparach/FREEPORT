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

    /// What the rock at `p` is made of: `TERRAIN` unless something built is
    /// the deepest solid there, which is how a mesher names a triangle.
    fn material(&self, _p: DVec3) -> u8 {
        TERRAIN
    }

    /// Whether the box from `lo` to `hi` is wholly rock (`Some(true)`),
    /// wholly air (`Some(false)`) or something the field cannot rule on
    /// (`None`), which is what lets a chunk with no surface in it be skipped
    /// without a sample. An answer must hold on the box's closed boundary,
    /// because a chunk beside a skipped one relies on the shared face having
    /// no crossing.
    fn solid(&self, _lo: DVec3, _hi: DVec3) -> Option<bool> {
        None
    }

    /// A bound on how fast the density can change, density per metre, or
    /// infinity where there is none. A sample of `v` a distance `d` from a
    /// point says the field there is within `slope * d` of `v`, which is
    /// what lets a chunk be ruled empty from a few samples.
    fn slope(&self) -> f64 {
        f64::INFINITY
    }
}

/// The nearest and farthest a box's points are from the origin.
pub fn box_radii(lo: DVec3, hi: DVec3) -> (f64, f64) {
    let near = DVec3::ZERO.clamp(lo, hi).length();
    let far = lo.abs().max(hi.abs()).length();
    (near, far)
}

/// The ground, whatever the planet is made of; the shader picks rock,
/// grass or sand by slope and height.
pub const TERRAIN: u8 = 0;
/// Poured concrete: what a block is made of.
pub const CONCRETE: u8 = 1;
/// Hull plate: rails, pillars, a cap.
pub const PLATE: u8 = 2;
/// A pane, dark.
pub const GLASS: u8 = 3;
/// A lamp: glows, and is a light.
pub const LAMP: u8 = 4;
/// A pane, lit from within.
pub const LIT: u8 = 5;
/// A street's paving.
pub const STREET: u8 = 6;

/// A ball of rock and nothing else.
pub struct Sphere {
    pub radius: f64,
}

impl Density for Sphere {
    fn at(&self, p: DVec3) -> f64 {
        self.radius - p.length()
    }

    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let (near, far) = box_radii(lo, hi);
        if far < self.radius {
            Some(true)
        } else if near > self.radius {
            Some(false)
        } else {
            None
        }
    }

    fn slope(&self) -> f64 {
        1.0
    }
}

/// A planet: a sphere with fractal relief on its surface and a little
/// three dimensional noise so a slope can overhang, and sites where the
/// relief is levelled for a town and the overhang faded out, or a plateau
/// would still undercut.
#[derive(Clone, Debug)]
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
    /// Where the ground is levelled for a town.
    pub sites: Vec<crate::town::Site>,
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
            sites: Vec::new(),
        }
    }
}

fn smoothstep(a: f64, b: f64, t: f64) -> f64 {
    let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// A site's levelling blends out to the relief over a skirt this far
/// inside and outside half its reach, metres.
const SKIRT_IN: f64 = 5.0;
const SKIRT_OUT: f64 = 6.0;

impl Planet {
    /// How much a site levels a direction: one right across it, nought
    /// past its apron.
    fn site_weight(&self, site: &crate::town::Site, dir: DVec3) -> f64 {
        let c = dir.dot(site.dir).clamp(-1.0, 1.0);
        let dist = c.acos() * self.radius;
        1.0 - smoothstep(site.r * 0.5 - SKIRT_IN, site.r * 0.5 + SKIRT_OUT, dist)
    }

    /// The relief at a direction, metres over the mean radius, sites
    /// applied, and how much of the overhang is kept there. On a levelled
    /// site the relief is the site's height and the noise is not asked.
    pub fn surface(&self, dir: DVec3) -> (f64, f64) {
        for site in &self.sites {
            if self.site_weight(site, dir) >= 1.0 {
                return (site.h, 0.0);
            }
        }
        let mut s =
            (fbm3(dir * self.lumps, self.seed, self.octaves) * 2.0 - 1.0) * self.relief * 0.5;
        let mut keep = 1.0;
        for site in &self.sites {
            let w = self.site_weight(site, dir);
            if w > 0.0 {
                s += (site.h - s) * w;
                keep *= 1.0 - w;
            }
        }
        (s, keep)
    }
}

impl Density for Planet {
    fn at(&self, p: DVec3) -> f64 {
        let r = p.length();
        if r == 0.0 {
            return self.radius;
        }
        let (surface, keep) = self.surface(p / r);
        let carve = if self.overhang > 0.0 && self.ledge > 0.0 && keep > 0.0 {
            (noise3(p / self.ledge, self.seed.wrapping_add(0x9E37)) - 0.5) * self.overhang * keep
        } else {
            0.0
        };
        self.radius + surface - r + carve
    }

    /// Outside the band the relief and the overhang can reach, the sign is
    /// the sphere's.
    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let (near, far) = box_radii(lo, hi);
        let (floor, top) = self.band();
        if far < floor {
            Some(true)
        } else if near > top {
            Some(false)
        } else {
            None
        }
    }

    fn slope(&self) -> f64 {
        self.steepest()
    }
}

impl Planet {
    /// The radii the surface stays between: the mean less and plus half the
    /// relief and half the overhang.
    pub fn band(&self) -> (f64, f64) {
        let reach = self.relief * 0.5 + self.overhang * 0.5;
        (self.radius - reach, self.radius + reach)
    }
}

/// The steepest `noise3` gets, per unit of its argument: a smoothstep
/// climbs at one and a half at most, across a unit cell, along each of
/// three axes.
const NOISE_SLOPE: f64 = 1.5 * 1.7320508;

impl Planet {
    /// One from the radius, the relief's fractal (each octave doubles the
    /// frequency and halves the weight, so every octave contributes the
    /// same slope) over the radius, the carve over its ledge, and across a
    /// site's skirt the blend from the relief to the site's level and of
    /// the carve to nought, which climbs at a smoothstep's one and a half
    /// over the skirt's width. The sites' skirts never overlap (`town::plan`
    /// keeps towns further apart than a site reaches), so the worst site
    /// bounds them all.
    fn steepest(&self) -> f64 {
        let octaves = self.octaves.max(1) as f64;
        let relief = self.relief * NOISE_SLOPE * self.lumps * octaves / self.radius;
        let carve = if self.ledge > 0.0 {
            self.overhang * NOISE_SLOPE / self.ledge
        } else {
            0.0
        };
        let level = self.sites.iter().map(|s| s.h.abs()).fold(0.0, f64::max);
        let skirt = if self.sites.is_empty() {
            0.0
        } else {
            (level + self.relief * 0.5 + self.overhang * 0.5) * 1.5 / (SKIRT_IN + SKIRT_OUT)
        };
        1.0 + relief + carve + skirt
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

/// A building standing on the planet: its recipe compiled, the frame it
/// stands in, and the box round it in the field's frame.
#[derive(Clone, Debug)]
pub struct Structure {
    pub frame: crate::town::Frame,
    pub building: crate::recipe::Building,
    pub lo: DVec3,
    pub hi: DVec3,
}

impl Structure {
    /// A building in a frame, its box the frame's image of the building's.
    pub fn new(frame: crate::town::Frame, building: crate::recipe::Building) -> Structure {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for c in 0..8 {
            let l = DVec3::new(
                if c & 1 != 0 {
                    building.hi.x
                } else {
                    building.lo.x
                },
                if c & 2 != 0 {
                    building.hi.y
                } else {
                    building.lo.y
                },
                if c & 4 != 0 {
                    building.hi.z
                } else {
                    building.lo.z
                },
            );
            let w = frame.world(l);
            lo = lo.min(w);
            hi = hi.max(w);
        }
        Structure {
            frame,
            building,
            lo,
            hi,
        }
    }

    /// Whether the box round the structure meets another box.
    pub fn meets(&self, lo: DVec3, hi: DVec3) -> bool {
        self.lo.cmple(hi).all() && self.hi.cmpge(lo).all()
    }

    /// Every lamp in the building, in the field's frame, with its reach.
    pub fn lamps(&self) -> Vec<(DVec3, f64)> {
        self.building
            .lamps
            .iter()
            .map(|(at, reach)| (self.frame.world(*at), *reach))
            .collect()
    }
}

/// Past this cell, metres, a building is its massing: on a lattice that
/// cannot hold a wall, a house is a block and a tower a drum.
pub const DETAIL: f64 = 0.5;

/// A field with things built on it: the ground, blocks ADDED to it in
/// order, each a union, and structures evaluated in their own frames, each
/// brush in list order. `cell` is what the field is being contoured on:
/// past `DETAIL` a structure is its massing.
pub struct Built<'a> {
    pub ground: &'a dyn Density,
    pub blocks: Vec<Block>,
    pub structures: Vec<&'a Structure>,
    pub cell: f64,
}

impl Built<'_> {
    /// What is at a point and what it is made of.
    pub fn sample(&self, p: DVec3) -> crate::recipe::Sample {
        let mut s = crate::recipe::Sample {
            d: self.ground.at(p),
            mat: TERRAIN,
            room: -1,
            curved: true,
        };
        for b in &self.blocks {
            let v = b.at(p);
            if v > s.d {
                s.d = v;
                s.mat = CONCRETE;
                s.curved = false;
            }
        }
        for st in &self.structures {
            if !(p.cmpge(st.lo).all() && p.cmple(st.hi).all()) {
                continue;
            }
            let local = st.frame.local(p);
            s = if self.cell > DETAIL {
                st.building.massing(local, s, self.cell)
            } else {
                st.building.sample(local, s)
            };
        }
        s
    }
}

impl Density for Built<'_> {
    fn at(&self, p: DVec3) -> f64 {
        self.sample(p).d
    }

    /// The deepest solid at `p`: a block whose density beats the ground's
    /// is what the rock there is made of, so a slab poured into a hillside
    /// meets the rock on a line and never as a blend.
    fn material(&self, p: DVec3) -> u8 {
        self.sample(p).mat
    }

    /// The ground's answer, unless a block or a structure reaches into the
    /// box.
    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let ground = self.ground.solid(lo, hi)?;
        let touched = self.blocks.iter().any(|b| {
            let (blo, bhi) = b.bounds();
            blo.cmple(hi).all() && bhi.cmpge(lo).all()
        }) || self.structures.iter().any(|st| st.meets(lo, hi));
        (!touched).then_some(ground)
    }

    /// A block is a distance, which climbs at one; the union climbs no
    /// faster than its steepest part.
    fn slope(&self) -> f64 {
        self.ground.slope().max(1.0)
    }
}

impl Block {
    /// The box round the block, in the field's frame.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        let c = self.corners();
        c.iter()
            .fold((c[0], c[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
    }

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
    fn a_float_hash_is_the_cores_to_a_hundred_millionth() {
        // A GPU has no f64, so `field.wgsl` computes `mix3` exactly and then
        // rounds it into a float's twenty four bits. That rounding is the
        // whole of the divergence between the ground a shader draws and the
        // ground the walker stands on, so it is measured rather than
        // assumed: the bound in metres is this share of the relief.
        let mut worst: f64 = 0.0;
        for i in 0..40i64 {
            for j in 0..40i64 {
                for k in 0..40i64 {
                    let a = hash3(i * 7 - 91, j * 13 - 17, k * 3 + 5, 11);
                    let b = hash3_f32(i * 7 - 91, j * 13 - 17, k * 3 + 5, 11) as f64;
                    worst = worst.max((a - b).abs());
                }
            }
        }
        // One part in 2^24, and a float cannot do better than half of that.
        assert!(worst < 6.0e-8, "the float hash is {worst} off");
        assert!(worst > 0.0, "a float held all thirty two bits?");
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
            structures: vec![],
            cell: 0.25,
        };
        assert_eq!(built.at(b.centre), 0.5);
        assert_eq!(built.at(DVec3::ZERO), 1.0);
        assert_eq!(built.material(b.centre), CONCRETE);
        assert_eq!(built.material(DVec3::ZERO), TERRAIN);
        assert_eq!(ground.material(DVec3::ZERO), TERRAIN);
    }

    #[test]
    fn a_box_is_ruled_rock_or_air_only_where_the_band_allows() {
        let planet = Planet {
            radius: 100.0,
            relief: 4.0,
            lumps: 3.0,
            octaves: 3,
            overhang: 1.0,
            ledge: 5.0,
            seed: 1,
            sites: vec![],
        };
        assert_eq!(planet.band(), (97.5, 102.5));
        let deep = (
            DVec3::new(-10.0, -10.0, -10.0),
            DVec3::new(10.0, 10.0, 10.0),
        );
        assert_eq!(planet.solid(deep.0, deep.1), Some(true));
        let high = (DVec3::new(0.0, 103.0, 0.0), DVec3::new(5.0, 110.0, 5.0));
        assert_eq!(planet.solid(high.0, high.1), Some(false));
        let crust = (DVec3::new(0.0, 95.0, 0.0), DVec3::new(5.0, 105.0, 5.0));
        assert_eq!(planet.solid(crust.0, crust.1), None);
        assert_eq!(box_radii(deep.0, deep.1), (0.0, (300.0f64).sqrt()));
        let ball = Sphere { radius: 5.0 };
        assert_eq!(
            ball.solid(DVec3::splat(6.0), DVec3::splat(7.0)),
            Some(false)
        );
        let slab = Block {
            centre: DVec3::new(0.0, 104.0, 0.0),
            half: DVec3::new(1.0, 1.0, 0.2),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
        };
        let built = Built {
            ground: &planet,
            blocks: vec![slab.clone()],
            structures: vec![],
            cell: 0.25,
        };
        assert_eq!(built.solid(high.0, high.1), None, "a slab in the box");
        // The slope bound holds: the field between two points a step apart
        // never changes faster than it says.
        let slope = planet.slope();
        assert!(slope > 1.0 && slope < 20.0, "slope {slope}");
        assert_eq!(built.slope(), slope);
        assert_eq!(ball.slope(), 1.0);
        let mut steepest: f64 = 0.0;
        for i in 0..2000 {
            let t = i as f64 * 0.37;
            let p = DVec3::new(t.sin() * 100.0, t.cos() * 100.0, (t * 0.3).sin() * 30.0);
            let step = DVec3::new(0.011, -0.007, 0.013);
            steepest = steepest.max((planet.at(p + step) - planet.at(p)).abs() / step.length());
        }
        assert!(
            steepest < slope,
            "measured {steepest} against the bound {slope}"
        );
        assert_eq!(built.solid(deep.0, deep.1), Some(true));
        let (lo, hi) = slab.bounds();
        assert!((lo - DVec3::new(-1.0, 103.8, -1.0)).length() < 1e-12);
        assert!((hi - DVec3::new(1.0, 104.2, 1.0)).length() < 1e-12);
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

    #[test]
    fn the_slope_bound_holds_across_a_sites_skirt() {
        let mut planet = Planet {
            radius: 2000.0,
            relief: 40.0,
            ..Default::default()
        };
        planet.sites = vec![crate::town::Site {
            dir: DVec3::Z,
            h: 30.0,
            r: 100.0,
        }];
        let bound = planet.slope();
        let mut worst = 0.0f64;
        for i in 0..400 {
            let dist = 35.0 + 35.0 * i as f64 / 400.0;
            let a = dist / planet.radius;
            let dir = DVec3::new(a.sin(), 0.0, a.cos());
            for k in 0..5 {
                let p = dir * (planet.radius - 20.0 + 15.0 * k as f64);
                for d in [DVec3::X, DVec3::Y, DVec3::Z] {
                    let h = 0.05;
                    let g = (planet.at(p + d * h) - planet.at(p - d * h)).abs() / (2.0 * h);
                    worst = worst.max(g);
                }
            }
        }
        assert!(
            worst <= bound,
            "the field climbs at {worst} against a bound of {bound}"
        );
        assert!(
            bound < worst * 8.0 + 2.0,
            "a bound of {bound} is slack against {worst}"
        );
    }
}
