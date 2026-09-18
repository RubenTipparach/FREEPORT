//! Towns: where they stand on a planet, and the lots and streets in them.
//!
//! `planTowns` from the mockup, ported. A town can stand on land a little
//! above the sea, on ground that is nearly level, and not on top of another
//! town. Candidates come off a golden angle spiral round the planet; the
//! LOWEST that qualifies is the port, because a port is the town this game
//! is about, and the rest are taken in a hashed order so a city is as
//! likely to stand inland or on an island as on a coast (`in_order`). A town is a local grid: blocks with streets between, a
//! lot per block, taller near the middle, a few blocks left as plazas; the
//! ground under it is levelled to its height by the planet's own field
//! (`Planet.sites`), and every building and every piece of street stands
//! plumb on its own patch of the sphere (`lot_frame`), so nothing long
//! enough for the ground to curve under it is placed in one piece.

use crate::field::{hash3, noise3, Density, Planet};
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
/// How far apart two towns stand over and above their own two radii,
/// metres, so a city and its neighbour have country between them.
const BETWEEN: f64 = 200.0;

/// How town SIZES are spread. Every town on a body used to be the one
/// figure across, so a hundred and sixty cities were a hundred and sixty
/// copies of one city; these are what make a few of them cities, some of
/// them towns and most of them villages.
///
/// ZIPF is the exponent of the rank size law, which is the one empirical
/// thing known about how big cities are: the nth biggest settlement in a
/// region is about `1/n^k` of the biggest, with k near one for a mature
/// country and lower where the ranking is looser. It is 0.32 here, which
/// over a hundred and sixty towns puts the smallest at a fifth of the
/// biggest rather than at a hundred and sixtieth, because a body with one
/// city and a hundred and fifty nine hamlets is a body with one place on
/// it worth landing at.
const ZIPF: f64 = 0.26;
/// The smallest a town may be as a share of the biggest, so the tail of
/// the law is a village rather than a single house.
const SMALLEST: f64 = 0.35;
/// How far a town's own hash moves its size either way, so two towns of
/// neighbouring rank are not the same town.
const SIZE_JITTER: f64 = 0.16;

/// A town's own OUTLINE, which is not a circle.
///
/// `LOBE` is how many lobes across the town the noise deciding its edge
/// has, and `REACH` how far that noise can push the edge either way as a
/// share of the nominal radius. A town is what grew where growing was
/// easy, so it reaches down one valley and stops short against whatever
/// was in the way on the other side: a disc reads as a stamp, and the
/// same disc at every one of a hundred and sixty sites reads as a stamp
/// used a hundred and sixty times.
const LOBE: f64 = 1.6;
const REACH: f64 = 0.42;

/// Where a town stops being one thing and starts being another, in the
/// same demand the outline is cut from: over `CORE_AT` is downtown and
/// over `TOWN_AT` is the town proper, and everything out to nought is
/// SUBURB.
const CORE_AT: f64 = 0.66;
const TOWN_AT: f64 = 0.38;

/// How many of a suburb's blocks carry a house at all. A suburb is a town
/// with SPACE in it, and what says so is the space rather than the house:
/// at one it is the same grid as downtown with shorter buildings on it,
/// which is what this was.
const SUBURB_FILL: f64 = 0.55;
/// How far a suburban house stands off the middle of its own block,
/// metres either way, against a town house's own small jitter. A setback
/// and a garden are the other thing that says suburb.
const SUBURB_SETBACK: f64 = 4.5;

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

/// The order qualifying sites are taken in: the PORT first, which is the
/// lowest ground on the body, and every other town on a HASH of its own
/// candidate index.
///
/// Sorted by HEIGHT, which is what this did, the lowest hundred and sixty
/// candidates win and every one of them is on a shore. Measured on the
/// harness planet: all 160 towns stood between 3 and 35 m over the sea on
/// a body with eight kilometres of relief and a two thousand metre
/// ceiling, so the interior of every continent was empty and the ceiling
/// the window was widened to had never once applied.
///
/// The spiral's OWN order is no better, because its index is monotone in
/// latitude by construction: the first hundred and sixty of it are a cap
/// round the north pole. A hash is an even sample of whatever qualified,
/// so a town is as likely to stand inland, on a plateau or on an island
/// as on a coast, in the proportion the body actually has of each.
fn in_order(mut cands: Vec<(DVec3, f64, usize)>, seed: u32) -> Vec<(DVec3, f64)> {
    let port = cands
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
        .map(|(at, _)| at);
    let first = port.map(|at| cands.remove(at));
    cands.sort_by(|a, b| {
        hash3(a.2 as i64, seed as i64, 11, seed).total_cmp(&hash3(
            b.2 as i64,
            seed as i64,
            11,
            seed,
        ))
    });
    first
        .into_iter()
        .chain(cands)
        .map(|(dir, h, _)| (dir, h))
        .collect()
}

/// How big the town of a given RANK on a body is, given the biggest.
///
/// Zipf's law with a floor and the town's own jitter (`ZIPF`, `SMALLEST`,
/// `SIZE_JITTER`). The rank is the town's own index, which is the order
/// `in_order` accepted it in, so the PORT is the biggest place on the
/// body and the rest descend: that is the one thing about rank here that
/// is not arbitrary, and it is the right thing, because a port is the
/// town this game is about.
///
/// It is a pure function of the index and the seed, which is what keeps
/// it out of the baked atlas: the file carries the biggest and every
/// town's own size is worked out again from its rank.
pub fn size_of(biggest: f64, index: usize, seed: u32) -> f64 {
    let rank = (index + 1) as f64;
    let zipf = rank.powf(-ZIPF).max(SMALLEST);
    let jitter = 1.0 + (hash3(index as i64, 17, 3, seed) - 0.5) * 2.0 * SIZE_JITTER;
    biggest * zipf * jitter
}

/// Whether the ground is level enough across a town of `radius` at a
/// direction: six samples out at half the radius, against a twelfth of it
/// in fall.
///
/// It is asked at ACCEPTANCE rather than in the candidate scan, and that
/// is what makes a town's size its own: a site has to be level across the
/// town that will actually stand on it, and which town that is depends on
/// its rank, which depends on what has been accepted already. It is also
/// far cheaper, because a scan asks this of twenty thousand candidates
/// and an acceptance loop asks it of the few hundred it looks at.
fn level_enough(planet: &Planet, sea: f64, dir: DVec3, h: f64, radius: f64) -> bool {
    let big_r = planet.radius;
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
    hi - lo <= radius * 0.12
}

/// Plan `count` towns on `planet`, the BIGGEST of them `biggest` across:
/// sites on land between `low` and `high` metres over the sea, level
/// across whatever town is going on them, apart from one another by both
/// their radii, the port first and the largest.
pub fn plan(planet: &Planet, sea: f64, biggest: f64, count: usize, seed: u32) -> Vec<Town> {
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
    let shape = planet.shape();
    let mut cands: Vec<(DVec3, f64, usize)> = Vec::new();
    for i in 0..CANDIDATES {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / CANDIDATES as f64;
        let s = (1.0 - y * y).sqrt();
        let a = golden * i as f64 + hash3(i as i64, seed as i64, 3, seed) * 0.3;
        let dir = DVec3::new(s * a.cos(), y, s * a.sin());
        let h = surface_radius(planet, dir) - sea;
        if !(low..=high).contains(&h) {
            continue;
        }
        // Nobody builds a city on an ICE CAP. The gate is the climate's
        // own `frozen`, which is the same threshold that paints the ice
        // and the snow, so a town can never stand on ground its own chart
        // draws white. A latitude would be a second number to get wrong,
        // and would still allow a city on a glacier at the equator.
        if shape.climate(dir, h).frozen() {
            continue;
        }
        cands.push((dir, h, i));
    }
    let cands = in_order(cands, seed);
    let mut towns: Vec<Town> = Vec::new();
    for (dir, h) in cands {
        if towns.len() >= count {
            break;
        }
        let radius = size_of(biggest, towns.len(), seed);
        // Apart by BOTH their radii, because they are not the same size
        // any more: one figure for the gap would stand a village as far
        // off its neighbour as a city stands off its own.
        if towns
            .iter()
            .any(|t| t.dir.dot(dir) > ((t.radius + radius + BETWEEN) / big_r).cos())
        {
            continue;
        }
        if !level_enough(planet, sea, dir, h, radius) {
            continue;
        }
        towns.push(lay(dir, h + sea - big_r, radius, towns.len(), seed));
    }
    towns
}

/// How much TOWN there is at a point of a town's own grid, metres east
/// and north of its middle: one at the very middle, nought at the edge,
/// and negative outside it.
///
/// It is the one number a town's shape is made of, and everything else is
/// read off it: where the town STOPS, which of the three zones a block is
/// in, and how tall what stands there is. One number with three
/// consequences rather than three rules that have to agree.
///
/// The LOBES are why an outline is not a circle. Two octaves of the
/// core's own value noise on the block's own place, a couple of lobes
/// across the town, pushing the edge in and out by `REACH` of the
/// nominal radius: a town is what grew where growing was easy, so it runs
/// a long way down one side and stops short on another. Circles at a
/// hundred and sixty sites read as one stamp used a hundred and sixty
/// times, which is exactly what the owner was looking at.
fn demand(x: f64, z: f64, radius: f64, seed: u32) -> f64 {
    let r = x.hypot(z) / radius.max(f64::MIN_POSITIVE);
    let p = DVec3::new(x / (radius * LOBE), 3.5, z / (radius * LOBE));
    let lobe = (noise3(p, seed) - 0.5) + (noise3(p * 2.7, seed ^ 0x5B2D) - 0.5) * 0.5;
    1.0 - r + lobe * REACH
}

/// What a block of a town's grid IS. The zones are the demand's own
/// thresholds, so a town's outline, its density and its skyline are three
/// readings of one field and cannot disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    /// Not town at all: country, and no block here.
    Away,
    /// Houses with space between them, one storey, set well back.
    Suburb,
    /// The town proper: streets of two and three storey buildings.
    Town,
    /// Downtown: the towers.
    Core,
}

impl Zone {
    fn of(want: f64) -> Zone {
        if want > CORE_AT {
            Zone::Core
        } else if want > TOWN_AT {
            Zone::Town
        } else if want > 0.0 {
            Zone::Suburb
        } else {
            Zone::Away
        }
    }
}

/// A town on a local grid: towers in the middle, streets of houses round
/// them, suburbs with gardens on the outside, and an outline that is not
/// a circle.
pub fn lay(dir: DVec3, h: f64, radius: f64, index: usize, seed: u32) -> Town {
    let (east, north) = frame_at(dir);
    // The grid reaches past the nominal radius, because the lobes do.
    let n = ((radius * (1.0 + REACH)) / PITCH).ceil() as i64;
    let seed = seed.wrapping_add(index as u32 * 977);
    let (lots, built) = plot(n, radius, seed);
    let pieces = streets_of(n, &built);
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

/// Which SIDES of a block want a street: west, east, south and north.
mod fronts {
    pub const WEST: u8 = 1;
    pub const EAST: u8 = 2;
    pub const SOUTH: u8 = 4;
    pub const NORTH: u8 = 8;
    pub const ALL: u8 = WEST | EAST | SOUTH | NORTH;
}

/// The one side a suburban block fronts: the one facing the middle of
/// town, so a run of them shares a road in and the road leads somewhere.
///
/// A suburb block used to front all four sides like a downtown one, and
/// a lone house with nothing built beside it then stood in a square ring
/// of its own tarmac. The owner would have read that off the picture as
/// a moat, and the picture is where it showed: the numbers said the town
/// had streets and it did.
fn faces(i: i64, j: i64) -> u8 {
    if i.abs() >= j.abs() {
        if i > 0 {
            fronts::WEST
        } else {
            fronts::EAST
        }
    } else if j > 0 {
        fronts::SOUTH
    } else {
        fronts::NORTH
    }
}

/// Which block of a town's grid carries what, and which sides of each
/// block want a street, which is what the streets are then laid along.
fn plot(n: i64, radius: f64, seed: u32) -> (Vec<Lot>, Vec<u8>) {
    let wide = (2 * n + 1) as usize;
    let mut built = vec![0u8; wide * wide];
    let mut lots = Vec::new();
    let hash = |i: i64, j: i64, k: i64| hash3(i, j, k, seed);
    for i in -n..=n {
        for j in -n..=n {
            let (cx, cz) = (i as f64 * PITCH, j as f64 * PITCH);
            let want = demand(cx, cz, radius, seed);
            let zone = Zone::of(want);
            if zone == Zone::Away {
                continue;
            }
            // A plaza downtown, a field in the suburbs: the same hash,
            // read against what the place can afford to leave empty.
            let empty = if zone == Zone::Suburb {
                1.0 - SUBURB_FILL
            } else {
                0.15
            };
            if hash(i, j, 0) < empty {
                continue;
            }
            let tall = match zone {
                // A suburb is ONE storey whatever the hash says. What
                // makes it a suburb is that nothing on it is tall.
                Zone::Suburb => 1,
                _ => 1 + ((hash(i, j, 3) * 0.4 + want * want) * 7.0).floor() as u32,
            };
            let (kind, storeys) = choose(tall, hash(i, j, 6));
            let jitter = if zone == Zone::Suburb {
                SUBURB_SETBACK * 2.0
            } else {
                BLOCK - 8.0
            };
            built[(i + n) as usize * wide + (j + n) as usize] = if zone == Zone::Suburb {
                faces(i, j)
            } else {
                fronts::ALL
            };
            lots.push(Lot {
                x: cx + (hash(i, j, 4) - 0.5) * jitter,
                z: cz + (hash(i, j, 5) - 0.5) * jitter,
                storeys,
                kind,
                id: ((i + 64) as u32) << 8 | (j + 64) as u32,
            });
        }
    }
    (lots, built)
}

/// The streets of a town: a piece wherever a street runs past a block
/// somebody built on, and nowhere else.
///
/// Laid over the whole disc instead, which is what this did, a town's
/// paving was a circle whatever shape the town itself came out, so the
/// outline the lobes cut was hidden under a perfectly round grid of
/// tarmac. A street that serves nothing is not a street.
fn streets_of(n: i64, built: &[u8]) -> Vec<Piece> {
    let wide = (2 * n + 1) as usize;
    let at = |i: i64, j: i64, side: u8| {
        (-n..=n).contains(&i)
            && (-n..=n).contains(&j)
            && built[(i + n) as usize * wide + (j + n) as usize] & side != 0
    };
    // Along the block's own edge: the line between block i - 1 and i.
    let line = |i: i64| i as f64 * PITCH - BLOCK / 2.0 - STREET / 2.0;
    let steps = (PITCH / PIECE).ceil() as i64;
    let mut pieces = Vec::new();
    for i in -n..=n + 1 {
        for j in -n..=n {
            // Every piece of this block's own frontage, so the paving is
            // continuous along a run of built blocks and stops with them.
            if !(at(i - 1, j, fronts::EAST) || at(i, j, fronts::WEST)) {
                continue;
            }
            for k in 0..steps {
                let mid = j as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * PIECE;
                pieces.push(Piece {
                    x: line(i),
                    z: mid,
                    w: STREET,
                    d: PIECE,
                });
            }
        }
    }
    for j in -n..=n + 1 {
        for i in -n..=n {
            if !(at(i, j - 1, fronts::NORTH) || at(i, j, fronts::SOUTH)) {
                continue;
            }
            for k in 0..steps {
                let mid = i as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * PIECE;
                pieces.push(Piece {
                    x: mid,
                    z: line(j),
                    w: PIECE,
                    d: STREET,
                });
            }
        }
    }
    pieces
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
mod tests;
