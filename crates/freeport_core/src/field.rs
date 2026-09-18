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

// The arithmetic the ground is made of lives in `noise` and is re-exported
// here, so every caller outside this crate keeps the path it had.
pub use crate::noise::{fbm3, hash3, hash3_f32, mix3, noise3, sample, Grid};

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

/// The arc from a site's middle inside which the ground is level right
/// across, and the arc past which it is the relief again, metres. The one
/// place the skirt's two widths are read, so a shader handed this pair is
/// applying the same rule `Planet::site_weight` does rather than a second
/// copy of two constants (`field.wgsl`'s `site_weight` is that shader).
pub fn site_band(site: &crate::town::Site) -> (f64, f64) {
    (site.r * 0.5 - SKIRT_IN, site.r * 0.5 + SKIRT_OUT)
}

impl Planet {
    /// How much a site levels a direction: one right across it, nought
    /// past its apron.
    fn site_weight(&self, site: &crate::town::Site, dir: DVec3) -> f64 {
        let (inner, outer) = site_band(site);
        let chord = (dir - site.dir).length();
        if chord * self.radius >= outer {
            return 0.0;
        }
        let dist = 2.0 * (chord * 0.5).clamp(0.0, 1.0).asin() * self.radius;
        1.0 - smoothstep(inner, outer, dist)
    }

    /// The additive site height and the fraction of procedural relief left
    /// at a direction. Shared with GPU input preparation so levelling has
    /// one definition and is still evaluated in the world frame's f64.
    pub fn surface_blend(&self, dir: DVec3) -> (f64, f64) {
        let (mut bias, mut keep) = (0.0, 1.0);
        for site in &self.sites {
            let w = self.site_weight(site, dir);
            if w >= 1.0 {
                return (site.h, 0.0);
            }
            bias += (site.h - bias) * w;
            keep *= 1.0 - w;
        }
        (bias, keep)
    }

    /// The relief at a direction, metres over the mean radius, sites
    /// applied, and how much of the overhang is kept there. On a levelled
    /// site the relief is the site's height and the noise is not asked.
    ///
    /// The relief itself is `biome::Shape::height`, which is a few terms
    /// of different characters rather than one fractal, and
    /// `sampling.wgsl` transcribes that function so the mesher's own
    /// samples and this one are the same ground.
    pub fn surface(&self, dir: DVec3) -> (f64, f64) {
        let (bias, keep) = self.surface_blend(dir);
        // A site CUTS and never FILLS: it takes whichever of its own
        // blend and the bare ground is LOWER.
        //
        // The blend on its own is a weighted average of the natural ground
        // and the site's level, so wherever the site stands over the land
        // it lifts it: a town on a slope came out on a pedestal with its
        // apron hanging out over the valley, which is a city that ADDED
        // ground rather than one that flattened it, and it is what the
        // owner read off the picture.
        //
        // A town's level is the LOWEST ground its own survey found
        // (`town::settle`), so inside the site the min is the site's level
        // and the platform is still flat; on the apron it is what stops
        // the skirt building land up. There is no fast path out of it: a
        // `keep == 0` branch that returned the level unconditionally put a
        // CLIFF round every site the moment the min was applied outside
        // and not inside, measured at a slope of 350 against a bound of
        // 30. One rule at every weight, and the bound holds because the
        // min of two functions is never steeper than the steeper of them.
        let bare = self.shape().height(dir);
        ((bias + bare * keep).min(bare), keep)
    }

    /// The terms this planet's relief is made of.
    pub fn shape(&self) -> crate::biome::Shape {
        crate::biome::Shape::of(self)
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

    /// Reject boxes using the radial band first, then a conservative local
    /// bound. Town skirts are kept unless a box lies wholly on a level site.
    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let (near, far) = box_radii(lo, hi);
        let (floor, top) = self.band();
        if far < floor {
            Some(true)
        } else if near > top {
            Some(false)
        } else {
            self.local_solid(lo, hi)
        }
    }

    fn slope(&self) -> f64 {
        self.steepest()
    }
}

impl Planet {
    fn local_solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let centre = (lo + hi) * 0.5;
        let reach = (hi - lo).length() * 0.5;
        let radius = centre.length();
        let near = radius - reach;
        if near <= 0.0 || !near.is_finite() {
            return None;
        }
        let dir = centre / radius;
        let span = (2.0 * reach / near + 1e-12).min(2.0);
        for site in &self.sites {
            let distance = (dir - site.dir).length();
            let (inner, outer) = site_band(site);
            if (distance - span) * self.radius >= outer {
                continue;
            }
            let level_chord = 2.0 * (0.5 * inner / self.radius).sin();
            if inner > 0.0 && distance + span < level_chord {
                // Wholly inside this site's LEVEL, where the ground is
                // `min(level, relief)`, because a site cuts and never
                // fills. So the level is an upper bound on the surface
                // and a box wholly outside the sphere at it is AIR.
                //
                // The other half of that answer is gone: calling a box
                // under the level ROCK was right while a site's ground
                // WAS its level, and is a hole in the world now, because
                // the cut can have taken that ground out from under it
                // and a chunk ruled rock is never meshed. What rules it
                // instead is the general path below, whose sample is the
                // real field and whose bound covers this too: the min of
                // a constant and the relief is never steeper than the
                // relief, and a level site carves nothing (`keep` is
                // nought there, so `at` adds no overhang).
                if let Some(false) = (Sphere {
                    radius: self.radius + site.h,
                })
                .solid(lo, hi)
                {
                    return Some(false);
                }
                break;
            }
            // Its skirt may cross the box. A planet-wide skirt slope is much
            // too loose to help, and overlapping sites must preserve order.
            return None;
        }
        // The direction's derivative is bounded by 1 / near throughout this
        // ball. Every octave's amplitude * frequency is one; ignoring their
        // normalization makes this conservative for any octave count.
        let relief =
            self.relief.abs() * NOISE_SLOPE * self.lumps.abs() * self.octaves.max(1) as f64 / near;
        let carve = if self.ledge > 0.0 {
            self.overhang.abs() * NOISE_SLOPE / self.ledge
        } else {
            0.0
        };
        let value = self.at(centre);
        let roundoff = 16.0 * f64::EPSILON * (self.radius.abs() + value.abs());
        (value.abs() > (1.0 + relief + carve) * reach + roundoff).then_some(value > 0.0)
    }

    /// This planet with only the sites whose levelling can reach inside
    /// `span`, a chord, of `dir`.
    ///
    /// A chunk's samples are all within a few metres of one another, so
    /// filtering ONCE per chunk turns a loop over every town on the planet
    /// into a loop over the nought or one that matter there. It is what
    /// makes a planet with cities all over it cost a chunk what a planet
    /// with eight does: `surface_blend` is asked for every one of a
    /// chunk's seven thousand sample points, and walking hundreds of
    /// sites in it is a million tests for a chunk that is nowhere near a
    /// town.
    pub fn around(&self, dir: DVec3, span: f64) -> Planet {
        if self.sites.is_empty() {
            return self.clone();
        }
        let mut local = self.clone();
        local.sites.retain(|site| {
            let (_, outer) = site_band(site);
            (dir - site.dir).length() - span <= outer / self.radius
        });
        local
    }

    /// The radii the surface stays between: what the relief's own terms can
    /// reach, and half the overhang either side of that. A site levels the
    /// ground to a height the relief already allowed, so it widens nothing.
    pub fn band(&self) -> (f64, f64) {
        let (floor, top) = self.shape().band();
        let carve = self.overhang * 0.5;
        (floor - carve, top + carve)
    }
}

/// The steepest `noise3` gets, per unit of its argument: a smoothstep
/// climbs at one and a half at most, across a unit cell, along each of
/// three axes.
const NOISE_SLOPE: f64 = 2.598_076_211_353_316;

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
        let relief = self.shape().slope();
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
    /// What it is made of, for a walker that asks what it is standing on.
    pub material: u8,
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

/// A field with things built on it: the ground, and the boxes of whatever
/// stands on it, each a union. What is built is a MODEL and not a brush
/// (`model.rs`), so a chunk is contoured on the ground alone and this is
/// what the WALKER walks: the boxes a model was drawn from, so the picture
/// and the collider are the same numbers.
pub struct Built<'a> {
    pub ground: &'a dyn Density,
    pub blocks: Vec<&'a Block>,
}

impl Built<'_> {
    /// The ground alone, which is what a chunk is contoured on.
    pub fn bare(ground: &dyn Density) -> Built<'_> {
        Built {
            ground,
            blocks: Vec::new(),
        }
    }

    /// What is at a point, and what it is made of: the DEEPEST solid, the
    /// one whose surface is farthest away, so a wall poured into a
    /// hillside meets the rock on a line and never as a blend.
    fn sample(&self, p: DVec3) -> (f64, u8) {
        let mut out = (self.ground.at(p), TERRAIN);
        for b in &self.blocks {
            let v = b.at(p);
            if v > out.0 {
                out = (v, b.material);
            }
        }
        out
    }
}

impl Density for Built<'_> {
    fn at(&self, p: DVec3) -> f64 {
        self.sample(p).0
    }

    fn material(&self, p: DVec3) -> u8 {
        if self.blocks.is_empty() {
            TERRAIN
        } else {
            self.sample(p).1
        }
    }

    /// The ground's answer, unless a block reaches into the box.
    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let ground = self.ground.solid(lo, hi)?;
        let touched = self.blocks.iter().any(|b| {
            let (blo, bhi) = b.bounds();
            blo.cmple(hi).all() && bhi.cmpge(lo).all()
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_bounds_reject_empty_boxes_inside_the_relief_band() {
        let planet = Planet {
            octaves: 18,
            overhang: 3.0,
            ledge: 12.0,
            sites: vec![crate::town::Site {
                dir: DVec3::Y,
                h: 0.0,
                r: 172.0,
            }],
            ..Planet::default()
        };
        for dir in [DVec3::Y, DVec3::X, DVec3::new(-0.6, 0.4, -0.7).normalize()] {
            let surface = crate::town::surface_radius(&planet, dir);
            for height in [-100.0, 100.0] {
                let centre = dir * (surface + height);
                let lo = centre - DVec3::splat(2.0);
                let hi = centre + DVec3::splat(2.0);
                assert_eq!(planet.solid(lo, hi), Some(height < 0.0));
                for k in 0..5 {
                    for j in 0..5 {
                        for i in 0..5 {
                            let p = lo + DVec3::new(i as f64, j as f64, k as f64);
                            assert_eq!(planet.at(p) > 0.0, height < 0.0);
                        }
                    }
                }
            }
            let centre = dir * surface;
            assert_eq!(planet.solid(centre - DVec3::ONE, centre + DVec3::ONE), None);
        }
    }

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
            material: CONCRETE,
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
            blocks: vec![&b],
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
        // The band's INVARIANT rather than its arithmetic: every direction's
        // ground is inside it, and it is not so wide that ruling is
        // pointless. The pair itself moved when the relief became a few
        // composed terms rather than one fractal, and a pin on the pair
        // would have read as a defect when what changed was the planet.
        let (floor, top) = planet.band();
        assert!(floor < planet.radius && top > planet.radius);
        assert!(
            top - floor < planet.relief * 2.0 + planet.overhang * 2.0,
            "the band {floor} to {top} is wider than the relief can reach"
        );
        for i in 0..2000 {
            let t = i as f64 * 0.618;
            let d = DVec3::new(t.sin(), (t * 0.37).cos(), (t * 1.3).sin()).normalize();
            let r = planet.radius + planet.surface(d).0;
            assert!(
                (floor..=top).contains(&r),
                "ground at {r} is outside the band {floor} to {top}"
            );
        }
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
            material: CONCRETE,
        };
        let built = Built {
            ground: &planet,
            blocks: vec![&slab],
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
    fn a_sites_band_is_level_inside_and_relief_outside() {
        let mut planet = Planet {
            radius: 2_000.0,
            relief: 40.0,
            lumps: 12.0,
            octaves: 8,
            overhang: 0.0,
            ledge: 0.0,
            seed: 3,
            sites: vec![],
        };
        let dir = DVec3::new(0.2, 0.9, 0.3).normalize();
        // Its level is UNDER the ground there, because a site cuts: one
        // above it is a site that does nothing, which is the other half
        // of this test.
        let site = crate::town::Site {
            dir,
            h: -12.0,
            r: 80.0,
        };
        let (inner, outer) = site_band(&site);
        assert!(inner < outer, "the band runs inward to outward");
        planet.sites = vec![site];
        let (east, _) = crate::town::frame_at(dir);
        // A point a hair inside the inner arc is the site's height and a
        // point a hair outside the outer one is the relief alone, which is
        // what a shader handed the pair has to reproduce.
        let at = |m: f64| {
            let a = m / planet.radius;
            (dir * a.cos() + east * a.sin()).normalize()
        };
        assert_eq!(planet.surface(at(inner - 0.5)).0, planet.sites[0].h);
        let bare = Planet {
            sites: vec![],
            ..planet.clone()
        };
        let far = at(outer + 0.5);
        assert!(
            (planet.surface(far).0 - bare.surface(far).0).abs() < 1e-12,
            "the ground past the skirt is not the relief"
        );
        // And a site standing OVER the ground changes nothing anywhere,
        // because a city flattens land and never adds it.
        let high = Planet {
            sites: vec![crate::town::Site {
                dir,
                h: 30.0,
                r: 80.0,
            }],
            ..bare.clone()
        };
        for m in [
            0.0,
            inner * 0.5,
            inner - 0.5,
            (inner + outer) * 0.5,
            outer + 0.5,
        ] {
            let d = at(m);
            assert!(
                (high.surface(d).0 - bare.surface(d).0).abs() < 1e-12,
                "a site over the ground raised it {m} m out"
            );
        }
    }

    /// A site's own skirt is inside the bound the mesher rules chunks on.
    ///
    /// Its level CUTS, and cuts the STEEPEST a site can: at 30 m on a body
    /// whose relief spans plus or minus twenty the site stood above every
    /// scrap of ground it covers, and a site that only ever takes the
    /// lower of itself and the land is then a site that does nothing at
    /// all. What a bound has to cover is the worst case, so the level is
    /// under the lowest ground here and the whole skirt is a cut.
    #[test]
    fn the_slope_bound_holds_across_a_sites_skirt() {
        let mut planet = Planet {
            radius: 2000.0,
            relief: 40.0,
            ..Default::default()
        };
        planet.sites = vec![crate::town::Site {
            dir: DVec3::Z,
            h: -25.0,
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
