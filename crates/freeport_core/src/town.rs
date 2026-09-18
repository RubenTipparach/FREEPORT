//! Towns: where they stand on a planet, and the lots and streets in them.
//!
//! `planTowns` from the mockup, ported. A town can stand on land a little
//! above the sea, on ground that is nearly level, and not on top of another
//! town. Candidates come off a golden angle spiral round the planet, and
//! the first that qualifies is the port, because a port is the town this
//! game is about. A town is a local grid: blocks with streets between, a
//! lot per block, taller near the middle, a few blocks left as plazas; the
//! ground under it is levelled to its height by the planet's own field
//! (`Planet.sites`), and every building and every piece of street stands
//! plumb on its own patch of the sphere (`lot_frame`), so nothing long
//! enough for the ground to curve under it is placed in one piece.

use crate::field::{hash3, Density, Planet};
use crate::model::Kind;
use glam::DVec3;

/// A place on the planet levelled for a town: its direction, its height
/// over the mean radius, and how far across the levelling reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub dir: DVec3,
    pub h: f64,
    pub r: f64,
}

/// A lot: where on the town's grid, metres east and north of its middle,
/// how big, how tall, and what kind of building stands on it.
#[derive(Clone, Debug)]
pub struct Lot {
    pub x: f64,
    pub z: f64,
    pub storeys: u32,
    pub kind: Kind,
    /// A number of its own, for what a model hashes.
    pub id: u32,
}

/// A piece of street: its middle, its size east and north, in town metres.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
}

/// A town: its place and frame on the sphere, its level, and its plan.
#[derive(Clone, Debug)]
pub struct Town {
    pub dir: DVec3,
    pub east: DVec3,
    pub north: DVec3,
    /// The town's level, metres over the mean radius.
    pub h: f64,
    pub radius: f64,
    pub lots: Vec<Lot>,
    pub pieces: Vec<Piece>,
    pub index: usize,
}

/// A frame on the sphere: a direction, east and north there, and the
/// radius its ground is at. Buildings are written in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub dir: DVec3,
    pub east: DVec3,
    pub north: DVec3,
    pub base: f64,
}

impl Frame {
    /// A world point in the frame: east, north, up from the ground.
    pub fn local(&self, p: DVec3) -> DVec3 {
        let d = p - self.dir * self.base;
        DVec3::new(d.dot(self.east), d.dot(self.north), d.dot(self.dir))
    }

    /// A frame point, east, north and up, in the world.
    pub fn world(&self, l: DVec3) -> DVec3 {
        self.dir * self.base + self.east * l.x + self.north * l.y + self.dir * l.z
    }
}

/// Blocks are this far apart, metres, and streets this wide.
pub const PITCH: f64 = 14.0;
pub const BLOCK: f64 = 10.0;
pub const STREET: f64 = 4.0;
/// A street is laid in pieces this long, each on its own patch.
pub const PIECE: f64 = 3.5;
/// How far the levelling reaches past a town's radius.
const APRON: f64 = 12.0;
/// How many directions are looked at for a town site. It is also the
/// densest cities can ever be: 20,000 points on a thousand kilometre
/// planet are about 25 km apart, which is a world with a town over most
/// horizons and wilderness between them.
const CANDIDATES: usize = 20_000;

/// East and north at a direction on the sphere.
pub fn frame_at(dir: DVec3) -> (DVec3, DVec3) {
    let up = dir.normalize();
    let pole = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let east = pole.cross(up).normalize();
    let north = up.cross(east).normalize();
    (east, north)
}

/// The smallest step the march down to the ground takes, metres: fine
/// enough that an overhang's ROOF is what is found and not the ground
/// under it, since the volumetric term's own features are a few metres.
const STEP: f64 = 0.5;

/// The radius at which the field first turns to rock coming in from space
/// along a direction: what a column of ground is high. The step it stops
/// on is refined by `ground_at`.
///
/// The march SPHERE TRACES on the field's own bound: a density of `v` in
/// the air is at least `-v / slope` from any crossing, so a step of that
/// cannot pass one, and `STEP` is only the floor under it. A fixed half
/// metre was the whole march before, which is one step per half metre of
/// BAND however wide the band is: sixty metres of relief is 120 samples a
/// direction and eight thousand is sixteen thousand, so `town::plan`,
/// which asks for seven of them at each of four thousand candidates, went
/// from 1.2 s on a ten kilometre planet to 15 s on a thousand kilometre
/// one. Measured over 400 directions, the same answer to the last bit
/// (nought of 400 differ, worst nought metres) for 15 ms against 1,760 on
/// the big planet and 9 against 15 on the small, and `plan` is 190 ms and
/// 579 ms.
pub fn surface_radius(planet: &Planet, dir: DVec3) -> f64 {
    let (bottom, top) = planet.band();
    let slope = planet.slope().max(f64::MIN_POSITIVE);
    let mut r = top;
    let mut last = top;
    while r > bottom {
        let v = planet.at(dir * r);
        if v > 0.0 {
            break;
        }
        last = r;
        r -= (-v / slope).max(STEP);
    }
    ground_at(planet, dir, r, last)
}

/// A lot's own frame on the sphere: its centre direction, east and north
/// there keeping the town's heading, and the town's level as its base.
pub fn lot_frame(planet_radius: f64, town: &Town, x: f64, z: f64) -> Frame {
    let dir =
        (town.dir + town.east * (x / planet_radius) + town.north * (z / planet_radius)).normalize();
    let east = (town.east - dir * town.east.dot(dir)).normalize();
    let north = dir.cross(east).normalize();
    Frame {
        dir,
        east,
        north,
        base: planet_radius + town.h,
    }
}

/// The site a town levels.
pub fn site_of(town: &Town) -> Site {
    Site {
        dir: town.dir,
        h: town.h,
        r: town.radius * 2.0 + APRON,
    }
}

/// Plan `count` towns of `radius` on `planet`: sites on land between
/// `low` and `high` metres over the sea, nearly level across, apart from
/// one another, the port first.
pub fn plan(planet: &Planet, sea: f64, radius: f64, count: usize, seed: u32) -> Vec<Town> {
    // No towns asked for is no candidates walked. The scan is four
    // thousand directions with a levelness test on each, and on a big
    // planet where none of them qualifies it is every one of them: nine
    // and a half seconds of looking for nought towns.
    if count == 0 {
        return Vec::new();
    }
    let big_r = planet.radius;
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    // How far over the sea a town may stand. The ceiling is the PLANET's,
    // not a fixed forty metres: on a world with eight kilometres of relief
    // a forty metre window is the coastal fringe and nothing else, so
    // every town came out on a beach and the interior of every continent
    // was empty. A city sits wherever the ground is level, and level
    // ground at two thousand metres is a plateau.
    let (low, high) = (3.0, (planet.relief * 0.3).max(40.0));
    let mut cands: Vec<(DVec3, f64)> = Vec::new();
    for i in 0..CANDIDATES {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / CANDIDATES as f64;
        let s = (1.0 - y * y).sqrt();
        let a = golden * i as f64 + hash3(i as i64, seed as i64, 3, seed) * 0.3;
        let dir = DVec3::new(s * a.cos(), y, s * a.sin());
        let h = surface_radius(planet, dir) - sea;
        if !(low..=high).contains(&h) {
            continue;
        }
        let (east, north) = frame_at(dir);
        let (mut lo, mut hi) = (h, h);
        for (e, n) in [
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (0.7, 0.7),
            (-0.7, -0.7),
        ] {
            let d = (dir + east * (e * radius * 0.5 / big_r) + north * (n * radius * 0.5 / big_r))
                .normalize();
            let hh = surface_radius(planet, d) - sea;
            lo = lo.min(hh);
            hi = hi.max(hh);
        }
        if hi - lo > radius * 0.12 {
            continue;
        }
        cands.push((dir, h));
    }
    cands.sort_by(|a, b| a.1.total_cmp(&b.1));
    let apart = ((2.0 * radius + 200.0) / big_r).cos();
    let mut towns: Vec<Town> = Vec::new();
    for (dir, h) in cands {
        if towns.len() >= count {
            break;
        }
        if towns.iter().any(|t| t.dir.dot(dir) > apart) {
            continue;
        }
        let index = towns.len();
        towns.push(lay(dir, h + sea - big_r, radius, index, seed));
    }
    towns
}

/// A town on a local grid: a lot per block, taller near the middle, a few
/// blocks left as plazas, and every street in pieces.
pub fn lay(dir: DVec3, h: f64, radius: f64, index: usize, seed: u32) -> Town {
    let (east, north) = frame_at(dir);
    let n = (radius / PITCH).floor() as i64;
    let hash = |i: i64, j: i64, k: i64| hash3(i, j, k, seed.wrapping_add(index as u32 * 977));
    let mut lots = Vec::new();
    for i in -n..=n {
        for j in -n..=n {
            let (cx, cz) = (i as f64 * PITCH, j as f64 * PITCH);
            if cx.hypot(cz) > radius {
                continue;
            }
            if hash(i, j, 0) < 0.15 {
                continue;
            }
            let near = 1.0 - cx.hypot(cz) / radius;
            let tall = 1 + ((hash(i, j, 3) * 0.4 + near * near) * 7.0).floor() as u32;
            let pick = hash(i, j, 6);
            let (kind, storeys) = choose(tall, pick);
            let jitter = BLOCK - 8.0;
            lots.push(Lot {
                x: cx + (hash(i, j, 4) - 0.5) * jitter,
                z: cz + (hash(i, j, 5) - 0.5) * jitter,
                storeys,
                kind,
                id: ((i + 64) as u32) << 8 | (j + 64) as u32,
            });
        }
    }
    let mut pieces = Vec::new();
    let reach = radius + STREET;
    let along = (reach / PIECE).ceil() as i64;
    for i in -n..=n + 1 {
        let at = i as f64 * PITCH - BLOCK / 2.0 - STREET / 2.0;
        for k in -along..along {
            let mid = (k as f64 + 0.5) * PIECE;
            if at.hypot(mid) > reach + PIECE {
                continue;
            }
            pieces.push(Piece {
                x: at,
                z: mid,
                w: STREET,
                d: PIECE,
            });
            pieces.push(Piece {
                x: mid,
                z: at,
                w: PIECE,
                d: STREET,
            });
        }
    }
    Town {
        dir,
        east,
        north,
        h,
        radius,
        lots,
        pieces,
        index,
    }
}

/// What kind of building a lot of a wanted height gets, and how many
/// storeys it ends up with: towers in the middle, one floor houses at the
/// edge, and the odd hangar among them.
fn choose(tall: u32, pick: f64) -> (Kind, u32) {
    if tall >= 4 {
        if pick < 0.6 {
            (Kind::Block, tall.clamp(4, 8))
        } else {
            (Kind::Tower, tall.clamp(3, 5))
        }
    } else if tall == 1 {
        if pick < 0.45 {
            (Kind::Bungalow, 1)
        } else if pick < 0.7 {
            (Kind::House, 1)
        } else {
            (Kind::Hangar, 1)
        }
    } else if pick < 0.85 {
        (Kind::House, tall)
    } else {
        (Kind::Block, tall.max(4))
    }
}

/// The radius where the field crosses between `near`, which is in rock,
/// and `far`, which is in air, along a direction: the one place a radius is
/// refined, by bisection.
pub fn ground_at(planet: &dyn Density, dir: DVec3, near: f64, far: f64) -> f64 {
    let (mut lo, mut hi) = (near, far);
    for _ in 0..40 {
        let m = 0.5 * (lo + hi);
        if planet.at(dir * m) > 0.0 {
            lo = m;
        } else {
            hi = m;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planet() -> Planet {
        Planet {
            radius: 1000.0,
            relief: 40.0,
            lumps: 6.0,
            octaves: 6,
            overhang: 1.0,
            ledge: 8.0,
            seed: 7,
            sites: vec![],
        }
    }

    #[test]
    fn towns_stand_on_level_land_over_the_sea_and_apart() {
        let planet = planet();
        let sea = 996.0;
        let towns = plan(&planet, sea, 40.0, 4, 7);
        assert_eq!(towns.len(), 4, "four sites on a small planet");
        for (i, t) in towns.iter().enumerate() {
            assert_eq!(t.index, i);
            let h = planet.radius + t.h - sea;
            assert!((3.0..=40.0).contains(&h), "town {i} at {h} m over the sea");
            assert!(t.lots.len() > 10, "{} lots", t.lots.len());
            assert!(t.pieces.len() > 40, "{} pieces of street", t.pieces.len());
            assert!(t.lots.iter().all(|l| l.x.hypot(l.z) <= t.radius + BLOCK));
            assert!(
                t.lots.iter().any(|l| l.storeys >= 4),
                "something tall in the middle"
            );
            assert!(
                t.lots.iter().any(|l| l.storeys == 1),
                "something low at the edge"
            );
            for u in towns.iter().skip(i + 1) {
                let apart = t.dir.angle_between(u.dir) * planet.radius;
                assert!(apart > 2.0 * 40.0 + 200.0, "towns {apart} m apart");
            }
        }
        // The port is the lowest.
        assert!(towns.windows(2).all(|w| w[0].h <= w[1].h));
        // A lot's frame is plumb where it stands and keeps the town's east.
        let t = &towns[0];
        let f = lot_frame(planet.radius, t, 30.0, -20.0);
        assert!((f.dir.length() - 1.0).abs() < 1e-12 && f.east.dot(f.dir).abs() < 1e-12);
        assert!(f.east.dot(t.east) > 0.99);
        let p = f.world(DVec3::new(1.0, 2.0, 3.0));
        assert!((f.local(p) - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
        assert!((f.local(f.dir * f.base)).length() < 1e-9);
        let site = site_of(t);
        assert_eq!(site.r, 2.0 * t.radius + APRON);
    }

    #[test]
    fn a_levelled_site_flattens_the_ground_to_the_towns_height() {
        let mut planet = planet();
        let sea = 996.0;
        let towns = plan(&planet, sea, 40.0, 2, 7);
        let t = &towns[0];
        let before = ground_at(&planet, t.dir, 950.0, 1050.0);
        planet.sites.push(site_of(t));
        let mid = ground_at(&planet, t.dir, 950.0, 1050.0);
        assert!(
            (mid - (planet.radius + t.h)).abs() < 0.05,
            "the middle at {} against the level {}",
            mid - planet.radius,
            t.h
        );
        assert!(
            (before - mid).abs() < 5.0,
            "the level is near the ground that was there"
        );
        // Right across the town the ground is at the level; well outside it
        // the ground is its own.
        let f = lot_frame(planet.radius, t, t.radius * 0.8, 0.0);
        let edge = ground_at(&planet, f.dir, 950.0, 1050.0);
        assert!(
            (edge - (planet.radius + t.h)).abs() < 0.05,
            "the edge at {}",
            edge - planet.radius
        );
        let far = (t.dir + t.east * (t.radius * 4.0 / planet.radius)).normalize();
        let mut bare = planet.clone();
        bare.sites.clear();
        assert!(
            (ground_at(&planet, far, 950.0, 1050.0) - ground_at(&bare, far, 950.0, 1050.0)).abs()
                < 1e-9
        );
    }
}
