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
/// The PAINT on a street: the centreline's dashes and the two lines
/// along the kerbs. It is a material rather than a mesh of its own,
/// because a marking is paint on a road and not a thing standing on
/// one: it wears the street's own set brightened, so it takes no
/// texture, no draw and no second shader.
pub const PAINT: u8 = 7;

/// What a building's own SKIN can be made of. A town used to be one grey:
/// every house and every office wore `CONCRETE`, so a suburb and a
/// downtown were the same wall at two heights, and the owner asked for
/// the trades' own list instead. `model::Kind::skin` is what picks one,
/// `tools/make_building_textures.py` bakes the set at the other end of
/// each byte, and `terrain.wgsl` draws them in the same triplanar pass
/// the ground and the concrete already go through: a wall costs no draw
/// call, no second shader and no material of its own, because what a
/// triangle is made of is one number on its own vertices.
///
/// Board and batten timber: a house.
pub const WOOD: u8 = 8;
/// Fired red brick in a running bond: a house or an office.
pub const BRICK: u8 = 9;
/// Vinyl lap siding, pale and near flat: a house.
pub const VINYL: u8 = 10;
/// Polished veined marble: an office that wants to be looked at.
pub const MARBLE: u8 = 11;
/// Dressed ashlar blocks: an office built to last.
pub const STONE: u8 = 12;
/// A glazed CURTAIN WALL: dark panes in a grid of aluminium mullions.
///
/// It is not `GLASS`, which is what a WINDOW is drawn with, and the
/// difference was measured in a picture: a whole tower given the pane's
/// material is a flat dark colour at a roughness of 0.08, which is a
/// mirror, and the first render of one reflected the sky so exactly
/// that the building vanished against it, leaving its own floor slabs
/// and window frames hanging in the air. A facade is a GRID, and the
/// grid is what makes it read as a building.
pub const CURTAIN: u8 = 13;

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
    pub sites: Sites,
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
            sites: Sites::default(),
        }
    }
}

use crate::noise::smoothstep;

/// How wide a site's levelling takes to blend out to the relief, metres.
/// Kept as two numbers because the slope bound below reads their sum and
/// a skirt that narrowed would be a cliff the mesher could not close.
const SKIRT_IN: f64 = 5.0;
const SKIRT_OUT: f64 = 6.0;

/// The arc from a site's middle inside which the ground is level right
/// across, and the arc past which it is the relief again, metres. The one
/// place the skirt's two widths are read, so a shader handed this pair is
/// applying the same rule `Planet::site_weight` does rather than a second
/// copy of two constants.
///
/// **`site.r` is the radius the ground is FULLY level inside**, and it
/// was half that. The first cut read `site.r * 0.5 - SKIRT_IN`, which is
/// the right band for a `site.r` that means a DIAMETER, and `site_of`
/// was handing it a radius: a town was levelled right across to about
/// its own nominal radius while its lots reach `town::OUTLINE` (2.06) of
/// one. Everything past that stood on bare relief with its base at the
/// town's level, and the level is the LOWEST of the site's own survey,
/// so the relief out there is HIGHER and the building is under it. The
/// owner's picture was a suburb buried to its eaves with only the roofs
/// and the driveways showing.
/// Every site on a body, kept so that the few which matter at a
/// direction can be found without walking the rest.
///
/// A body with eight towns on it could be walked; a body with a hundred
/// and sixty could, once `Planet::around` filtered once a CHUNK rather
/// than once a sample. A body whose ROADS are levelled cannot: this one
/// carries 1,084 settlements and 310 roads over 63,840 km, and a
/// corridor cut every few hundred metres is hundreds of thousands of
/// sites. `surface_blend` is asked for every one of a chunk's seven
/// thousand sample points, so walking them all is a hundred million
/// tests for a chunk in the middle of an ocean.
///
/// The index is the simplest one that works on a sphere and keeps this
/// crate's no-`HashMap` rule: the sites SORTED BY LATITUDE, which is
/// `dir.y` because that is what `biome` already measures latitude on,
/// and one number for how far the widest of them reaches off its own.
/// A query is a binary search and a walk of a thin band. It is not a
/// quadtree because it does not need to be: a band of the sphere a few
/// hundred metres deep holds a handful of sites out of any number.
#[derive(Clone, Debug, Default)]
pub struct Sites {
    /// Sorted by `mid`, so a query is a range.
    by_lat: Vec<crate::town::Site>,
    /// How far the widest site reaches off its own middle latitude,
    /// as a share of the y axis (half its arc) and in metres (its outer
    /// band). `window` is what turns the pair into one number.
    reach_y: f64,
    reach_m: f64,
    /// The worst level and the worst DROP along an arc, which is what
    /// the field's slope bound reads. Precomputed, because the bound is
    /// asked once a box and the sites are not walked for it.
    level: f64,
    drop: f64,
    /// The shortest arc any site spans, radians, so the steepest grade
    /// is bounded by `drop / (sweep * radius)`.
    sweep: f64,
}

impl Sites {
    /// The index over a list of sites.
    pub fn new(sites: Vec<crate::town::Site>) -> Sites {
        let mut out = Sites {
            by_lat: sites,
            sweep: f64::INFINITY,
            ..Sites::default()
        };
        for site in &out.by_lat {
            out.reach_y = out.reach_y.max((site.dir.y - site.to.y).abs() * 0.5);
            out.reach_m = out.reach_m.max(site_band(site).1);
            out.level = out.level.max(site.h.abs()).max(site.to_h.abs());
            out.drop = out.drop.max((site.to_h - site.h).abs());
            let sweep = site.dir.angle_between(site.to);
            if sweep > 0.0 {
                out.sweep = out.sweep.min(sweep);
            }
        }
        out.by_lat.sort_by(|a, b| {
            Sites::mid(a)
                .partial_cmp(&Sites::mid(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    /// A site's own middle latitude, which is what it is sorted on.
    fn mid(site: &crate::town::Site) -> f64 {
        (site.dir.y + site.to.y) * 0.5
    }

    pub fn is_empty(&self) -> bool {
        self.by_lat.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_lat.len()
    }

    /// Every site, in no useful order.
    pub fn iter(&self) -> std::slice::Iter<'_, crate::town::Site> {
        self.by_lat.iter()
    }

    /// No sites at all.
    pub fn clear(&mut self) {
        *self = Sites::default();
    }

    /// One more site. It re-sorts, so it is for building a body and not
    /// for a loop.
    pub fn push(&mut self, site: crate::town::Site) {
        let mut all = std::mem::take(&mut self.by_lat);
        all.push(site);
        *self = Sites::new(all);
    }

    /// How far off a query's own latitude a site can still reach, on a
    /// body of this radius.
    pub fn window(&self, radius: f64) -> f64 {
        self.reach_y
            + if radius > 0.0 {
                self.reach_m / radius
            } else {
                0.0
            }
    }

    /// The highest level any site holds, metres, and the steepest its
    /// own level ramps ALONG it. A town's ramp is nought; a road's is
    /// the grade it was routed at, and the field's slope bound has to
    /// carry it or a chunk on a hill road is ruled empty and left as a
    /// hole.
    pub fn level(&self) -> f64 {
        self.level
    }

    pub fn grade(&self, radius: f64) -> f64 {
        if self.sweep.is_finite() && self.sweep > 0.0 && radius > 0.0 {
            self.drop / (self.sweep * radius)
        } else {
            0.0
        }
    }

    /// The sites whose own latitude band is within `window` of a
    /// direction's: a binary search and a walk. Everything else on the
    /// body is skipped without being looked at.
    pub fn near(&self, dir: DVec3, window: f64) -> impl Iterator<Item = &crate::town::Site> {
        let (lo, hi) = (dir.y - window, dir.y + window);
        let start = self.by_lat.partition_point(|s| Sites::mid(s) < lo);
        self.by_lat[start..]
            .iter()
            .take_while(move |s| Sites::mid(s) <= hi)
    }
}

impl<'a> IntoIterator for &'a Sites {
    type Item = &'a crate::town::Site;
    type IntoIter = std::slice::Iter<'a, crate::town::Site>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::ops::Index<usize> for Sites {
    type Output = crate::town::Site;
    /// One site, in the index's own order, which is by latitude and not
    /// the order they were handed in. It is here for the tests that hold
    /// a body with exactly one site on it.
    fn index(&self, k: usize) -> &crate::town::Site {
        &self.by_lat[k]
    }
}

impl FromIterator<crate::town::Site> for Sites {
    fn from_iter<T: IntoIterator<Item = crate::town::Site>>(sites: T) -> Sites {
        Sites::new(sites.into_iter().collect())
    }
}

impl From<Vec<crate::town::Site>> for Sites {
    fn from(sites: Vec<crate::town::Site>) -> Sites {
        Sites::new(sites)
    }
}

pub fn site_band(site: &crate::town::Site) -> (f64, f64) {
    (site.r, site.r + SKIRT_IN + SKIRT_OUT)
}

impl Planet {
    /// How much a site levels a direction: one right across it, nought
    /// past its apron.
    fn site_weight(&self, site: &crate::town::Site, dir: DVec3) -> f64 {
        let (inner, outer) = site_band(site);
        // To the NEAREST point of the arc, which is the site's own middle
        // on a town and a point along the corridor on a road.
        let chord = (dir - site.nearest(dir).0).length();
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
        let (bias, keep, _) = self.levelling(dir);
        (bias, keep)
    }

    /// The same, and how much of the levelling here may BUILD GROUND UP:
    /// nought under a town, which may only cut, and one under a road's
    /// corridor, which is built on cut and fill like any road.
    ///
    /// It is the same loop rather than a second one, because the two
    /// answers come off the same weights and a caller that asked twice
    /// would walk every site twice.
    ///
    /// **A point several sites cover outright takes the NEAREST one's
    /// level**, and that is not a tie break, it is a road's own profile.
    /// A corridor is a chain of arcs whose ends MEET, and an arc's band
    /// is a capsule: its round end reaches `CORRIDOR` metres past its own
    /// last station into its neighbour. Returning on the first site the
    /// index reached gave a point seven metres past a station the level
    /// AT that station rather than the ramp, so every station on a road
    /// carried a fourteen metre landing and a one in ten grade stepped
    /// 0.66 m at each of them. Which of the two arcs won was the latitude
    /// sort's business, so the landings were not even consistent. The
    /// point seven metres along the next arc stands ON that arc's axis
    /// and seven metres off the last one's, so the nearest is the one
    /// whose ramp it is, and the profile is continuous by construction.
    pub fn levelling(&self, dir: DVec3) -> (f64, f64, f64) {
        let (mut bias, mut keep, mut fill) = (0.0f64, 1.0f64, 0.0f64);
        // The site that covers this point OUTRIGHT, and how far its own
        // axis is: several can cover one point, so which one's level is
        // the ground here has to be decided rather than taken from
        // whichever the index happened to reach first.
        let (mut covered, mut nearest) = (None, f64::MAX);
        for site in self.sites_near(dir, 0.0) {
            let w = self.site_weight(site, dir);
            if w <= 0.0 {
                continue;
            }
            // The level where the ARC is nearest, which ramps along a
            // road's corridor and is constant across a town.
            let (at, level) = site.nearest(dir);
            if site.fills {
                fill = fill.max(w);
            }
            if w >= 1.0 {
                let away = (dir - at).length_squared();
                if away < nearest {
                    (nearest, covered) = (away, Some(level));
                }
                continue;
            }
            bias += (level - bias) * w;
            keep *= 1.0 - w;
        }
        match covered {
            Some(level) => (level, 0.0, fill),
            None => (bias, keep, fill),
        }
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
        let (bias, keep, fill) = self.levelling(dir);
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
        //
        // A ROAD may fill, and `fill` is how much of the levelling here
        // is a road's: nought takes the min as ever, one takes the ramp
        // whether the ground under it is higher or lower, and the band
        // between is continuous, so a corridor's own skirt rises out of
        // a hollow rather than stepping out of it. A road laid on a
        // corridor that could only cut floated 18.6 m over the ground in
        // the worst place on this body, because the ramp between two
        // stations runs over every dip between them; an embankment is
        // the other half of a cutting.
        let bare = self.shape().height(dir);
        let graded = bias + bare * keep;
        (graded.min(bare) + (graded - graded.min(bare)) * fill, keep)
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
        for site in self.sites_near(dir, span) {
            let distance = (dir - site.nearest(dir).0).length();
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
                // The HIGHER end of the arc, which is the upper bound
                // on the surface all along it: a lower one would call a
                // box air that the corridor's own ramp still reaches.
                if let Some(false) = (Sphere {
                    radius: self.radius + site.h.max(site.to_h),
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
            return self.bare();
        }
        let kept: Vec<_> = self
            .sites_near(dir, span)
            .filter(|site| {
                let (_, outer) = site_band(site);
                (dir - site.nearest(dir).0).length() - span <= outer / self.radius
            })
            .copied()
            .collect();
        Planet {
            sites: Sites::new(kept),
            ..self.bare()
        }
    }

    /// This body with NO sites on it, and nothing else copied that is not
    /// a number.
    ///
    /// `..self.clone()` is what this replaces and it was the whole cost
    /// of a road: struct update syntax evaluates the base FIRST, so
    /// `Planet { sites, ..self.clone() }` clones every site on the body
    /// and then throws the list away. With eight towns that is a hundred
    /// bytes and nobody notices; with 190,168 levelled corridor pieces it
    /// is 13 MB a call, and `around` is called once a CHUNK and once a
    /// sample in the bake's own survey. Measured on the port: 71 ms a
    /// chunk against 8, and a bake of 43 s against 24.
    pub fn bare(&self) -> Planet {
        Planet {
            radius: self.radius,
            relief: self.relief,
            lumps: self.lumps,
            octaves: self.octaves,
            overhang: self.overhang,
            ledge: self.ledge,
            seed: self.seed,
            sites: Sites::default(),
        }
    }

    /// The sites whose latitude band can reach a direction: the index's
    /// own window widened by however far the box being asked about
    /// spans. Everything else on the body is never looked at.
    fn sites_near(&self, dir: DVec3, span: f64) -> impl Iterator<Item = &crate::town::Site> {
        self.sites.near(dir, span + self.sites.window(self.radius))
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

/// How many sites' skirts the slope bound assumes can cross one point.
/// See `steepest` for why it is not one any more, and why it is TWO
/// rather than the count of roads that can meet at a town.
const OVERLAP: f64 = 2.0;

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
        let skirt = if self.sites.is_empty() {
            0.0
        } else {
            // OVERLAP, because a road's corridor is a chain of arcs
            // whose ends MEET, so their bands always overlap and
            // `town::plan`'s old promise that no two skirts cross is
            // gone. The blend's derivative is bounded by the sum of the
            // overlapping sites' terms, so in principle the bound wants
            // their count, which at a hub is the town's disc plus the
            // first arc of every road that reaches it.
            //
            // It is TWO, because of what those sites AGREE about:
            // `road::corridor` starts each arc on the level the last one
            // ended at and ends the chain on the town's own level, so
            // every pair that overlaps carries the same level where they
            // overlap and the composed blend has no step in it to climb.
            // What is left is the pair at a join, and two is the honest
            // bound for that. Eight, which is the count, made the bound
            // 81.6 against a measured worst of 5.7 and
            // `the_slope_bound_holds_across_a_sites_skirt` says so: a
            // bound too tight rules a chunk empty that has surface in it,
            // which is a hole in the world, and one too loose costs a
            // sample on every chunk of the body.
            (self.sites.level() + self.relief * 0.5 + self.overhang * 0.5) * 1.5 * OVERLAP
                / (SKIRT_IN + SKIRT_OUT)
        };
        // And the corridor's own grade ALONG itself, which a town does
        // not have: a road climbs at up to `road::STEEPEST` and the
        // ground under it climbs with it.
        1.0 + relief + carve + skirt + self.sites.grade(self.radius)
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
mod tests;
