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

use crate::field::{hash3, Density, Planet};
use crate::model::Kind;
use glam::{DVec2, DVec3};

/// A lot: where on the town's grid, metres east and north of its middle,
/// how big, how tall, and what kind of building stands on it.
#[derive(Clone, Debug)]
pub struct Lot {
    pub x: f64,
    pub z: f64,
    /// Its FOOTPRINT, metres a side: one `LOT`, or two of them for the
    /// large buildings downtown and round the square.
    pub w: f64,
    /// How far its own frame is turned about the up, radians, so its
    /// DOOR faces the street it fronts: a lot on the north side of a
    /// block opens north, and a building drawn with its door on its
    /// own south wall is turned half round to do it.
    pub yaw: f64,
    pub storeys: u32,
    pub kind: Kind,
    /// A number of its own, for what a model hashes.
    pub id: u32,
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
    /// Which way the town GREW, east and north in its own frame: along
    /// its own shore, which is across the way the land falls. Nought on
    /// ground with no slope to it, and the town is then round.
    pub along: DVec2,
    pub lots: Vec<Lot>,
    pub pieces: Vec<Piece>,
    pub index: usize,
    /// The town's OWN seed, `town_seed` of the world's and its index:
    /// what its lobes, its plazas and its skins are drawn off.
    pub seed: u32,
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

    /// The same frame turned `yaw` radians about its own up, anticlockwise
    /// seen from above: what a lot's model is written in so its door
    /// faces the street it fronts.
    pub fn turned(&self, yaw: f64) -> Frame {
        let (s, c) = yaw.sin_cos();
        Frame {
            dir: self.dir,
            east: (self.east * c + self.north * s).normalize_or(self.east),
            north: (self.north * c - self.east * s).normalize_or(self.north),
            base: self.base,
        }
    }
}

/// How far the levelling reaches past a town's outline: half a block and
/// a street, which is how far the far kerb of the outermost street can
/// stand past the middle of the outermost block. It was twelve metres
/// when a block was ten, and at forty the first street laid past the
/// edge stood on bare relief eight centimetres over the town's level.
const APRON: f64 = BLOCK * 0.5 + STREET;
/// How many directions are looked at for a town site. It is also the
/// densest cities can ever be: 20,000 points on a thousand kilometre
/// planet are about 25 km apart, which is a world with a town over most
/// horizons and wilderness between them.
const CANDIDATES: usize = 20_000;
/// How far apart two towns stand over and above their own two radii,
/// metres, so a city and its neighbour have country between them.
pub(crate) const BETWEEN: f64 = 200.0;

/// How town SIZES are spread. Every town on a body used to be the one
/// figure across, so a hundred and sixty cities were a hundred and sixty
/// copies of one city; these are what make a few of them cities, some of
/// them towns and most of them villages.
///
/// How fast a town's size falls off as it stands further from the sea,
/// as a share of the body's own RADIUS: the distance from the water at
/// which a town is halfway down to the smallest. Three hundredths is
/// thirty kilometres on the harness planet, which is where its land's
/// MEDIAN distance from the sea falls (the bake prints it: 31 km), so
/// half the interior is nearer the coast than the halfway point and
/// half is further, which is what spreads the sizes; at a tenth the
/// bake came back with 199 settlements against 521, because nearly
/// every site was asked to be a city and a city needs a plain. It is
/// thirty metres on a one kilometre test ball. Measured in the body's
/// own size rather than in metres, so the law spreads the same way on
/// any body.
///
/// A share of the RELIEF is what this was first, and then a share of the
/// habitable WINDOW, and both were reading the HEIGHT over the sea as a
/// stand in for the distance to it. `shore::Shore` is the distance
/// itself.
const COAST: f64 = 0.03;
/// How sharply it falls: over one at the shore and flat inland, which is
/// the shape of what a port is worth against what a market town is.
const COAST_BIAS: f64 = 1.7;
/// The smallest a town may be as a share of the biggest, which is what an
/// inland one settles at.
const SMALLEST: f64 = 0.32;
/// How far a town's own hash moves its size either way, so two towns on
/// one shore are not twins.
const SIZE_JITTER: f64 = 0.16;
/// How far a site may FALL across itself, as a share of the town's own
/// radius. It is what the grading has to cut away, and every metre of it
/// is a step down at the town's rim: at a tenth a hundred and fifty metre
/// city is cut fifteen metres into its own hill, which reads as a
/// terrace, and much more than that reads as a quarry.
const LEVEL: f64 = 0.10;
/// And the DEEPEST a site may cut whatever its size, metres.
///
/// `LEVEL` is a share of the town's own radius, so it says the cut
/// grows with the town, and its own doc says what that is for: "at a
/// tenth a hundred and fifty metre city is cut fifteen metres into its
/// own hill, which reads as a terrace, and much more than that reads as
/// a quarry". A city is 537 m of radius now rather than 170, so that
/// share is a 105 m cut at the worst and 18 m at the mean, measured on
/// this body: the constant's own doc condemned it seven times over the
/// day the towns grew.
///
/// A cut is one number a share cannot express, because what says
/// terrace or quarry is the WALL in metres against the buildings beside
/// it: fifteen metres is five storeys and is a retaining wall a town
/// has, and a hundred is a cliff nothing built explains. It is a
/// maximum rather than a minimum, so a body whose towns are small never
/// meets it: on a two kilometre ball with twelve metres of relief the
/// share binds first and this never does.
///
/// It is what makes a town's own apron CLIMBABLE, which is the other
/// half. `field::site_skirt` ramps the cut over 24.6 m and a smoothstep
/// climbs at one and a half its average, so a 15 m cut is an apron of
/// 0.91 against `walker::STAND`'s own 1.19: a walker gets up it and so
/// does a car, which is this game's own rule that the two are one
/// answer. At 105 m it was 6.41, which is a wall.
const CUT: f64 = 15.0;

/// How much of its own size a settlement dropped on a ROAD gets. A place
/// that grew because the road goes past it is a village whatever its
/// shore, so the coastal law still shapes it and this is what keeps it
/// from competing with the cities the road joins.
pub(crate) const WAYSIDE: f64 = 0.42;

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
    surface_radius_from(planet, dir, planet.band().1)
}

/// The same march, started from a radius the caller already KNOWS is in
/// the air.
///
/// The band is the whole of what the relief can reach, which on this
/// body is sixteen kilometres, and the march's step is floored at
/// `STEP`: a caller with no idea where the ground is pays for all of
/// it. A caller that has the ANALYTIC surface in hand does know, within
/// `Planet::overhang`, because the volumetric term is the only thing
/// the analytic answer leaves out and it can lift the surface by half
/// of that at the most. Measured on the road's own slips, 14,310
/// marches took **74.6 s of startup from the top of the band and 0.3 s
/// from the analytic surface plus the overhang**, for the same answer.
///
/// `top` under the true surface is the one way to get this wrong, and
/// what comes back then is `top` itself rather than a wrong crossing,
/// which is why the margin is the whole overhang and not half of it.
pub fn surface_radius_from(planet: &Planet, dir: DVec3, top: f64) -> f64 {
    let (bottom, _) = planet.band();
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

/// The site a town levels: the ground it stands on, right across its own
/// OUTLINE and an apron past that.
///
/// `OUTLINE` and not one radius. A town reaches 2.06 of its nominal
/// radius (the lobes times the stretch) and `lay` emits lots all the way
/// out there, so a site written against the radius alone levelled about
/// the middle half of the town and left the suburbs on bare relief. The
/// apron covers the half block and half street a lot's own corner stands
/// past its centre.
pub fn site_of(town: &Town) -> Site {
    Site::town(town.dir, town.h, town.radius, town.along, town.seed)
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

/// How big a town standing `shore` metres from the SEA is, as a share
/// of the biggest on the body.
///
/// **A big city is COASTAL.** That is the owner's own observation and it
/// is most of economic geography: a port trades with the whole world and
/// an inland town with its own valley, so the great cities are on water
/// and the interior carries market towns and villages. What it replaces
/// was Zipf on the town's RANK, which gives the right spread of sizes and
/// puts them in an arbitrary place: the biggest city on the body was
/// wherever the hash happened to accept first.
///
/// The measure is the DISTANCE to the nearest water (`Shore`), and it
/// was the height over the sea for a long time on the reasoning that
/// the continent term is a plateau with a steep shelf, so low ground IS
/// the coastal fringe. Measured on the atlas that gave: not one of 521
/// settlements with open water within three kilometres and the port's
/// own nearest sea 15.7 km off, because `CUT` caps how deep a site may
/// cut and the flattest big sites are inland basins. A proxy that
/// disagrees with the thing it stands for is what the owner read off
/// the map as cities that were not on the coast.
pub fn coastal(shore: f64, planet: &Planet) -> f64 {
    let reach = (planet.radius * COAST).max(f64::MIN_POSITIVE);
    let out = (shore / reach).max(0.0);
    SMALLEST + (1.0 - SMALLEST) / (1.0 + out.powf(COAST_BIAS))
}

/// A town's own size, metres: the coastal share of the biggest on the
/// body, with its own jitter so two towns on one shore are not twins.
pub fn size_of(biggest: f64, shore: f64, planet: &Planet, index: usize, seed: u32) -> f64 {
    let jitter = 1.0 + (hash3(index as i64, 17, 3, seed) - 0.5) * 2.0 * SIZE_JITTER;
    biggest * coastal(shore, planet) * jitter
}

/// What the natural ground does across a town's own site.
struct Ground {
    /// The LOWEST it reaches, metres over the sea.
    low: f64,
    /// How far it falls from end to end.
    fall: f64,
    /// Which way it falls, in the site's own east and north: downhill,
    /// which on a coastal site is the way the water is.
    down: DVec2,
}

/// How many bearings the site is sampled on, and at what shares of the
/// town's own radius. Two rings rather than one, because a site that is
/// level across its middle and falls off a cliff at its rim is a site
/// whose town stands on a pedestal.
const BEARINGS: usize = 12;
/// The rings the survey walks, as shares of the town's own OUTLINE, so
/// the level it settles on is the lowest of the ground the town ACTUALLY
/// covers. They were shares of the nominal radius out to 1.05, which is
/// half a town: the level was the lowest of the middle and the ground
/// the suburbs stood on had never been looked at.
const RINGS: [f64; 4] = [0.25 * OUTLINE, 0.5 * OUTLINE, 0.75 * OUTLINE, OUTLINE];

/// What the ground does across a site, in `BEARINGS` times `RINGS`
/// samples plus the middle: forty nine marches.
///
/// Its LOWEST is what the town's level becomes, so the residual is a dip
/// between two neighbouring samples, and how deep that can be is bounded
/// by the `LEVEL` fall the site had to pass to be accepted at all.
///
/// It is asked at ACCEPTANCE rather than in the candidate scan, and that
/// is what makes a town's size its own: a site has to be level across the
/// town that will actually stand on it. It is also far cheaper, because a
/// scan asks this of twenty thousand candidates and an acceptance loop
/// asks it of the few hundred it looks at.
fn site_ground(planet: &Planet, sea: f64, dir: DVec3, h: f64, radius: f64) -> Ground {
    let big_r = planet.radius;
    let (east, north) = frame_at(dir);
    let (mut lo, mut hi) = (h, h);
    let mut down = DVec2::ZERO;
    for k in 0..BEARINGS {
        let a = std::f64::consts::TAU * k as f64 / BEARINGS as f64;
        let (e, n) = (a.cos(), a.sin());
        for reach in RINGS {
            let step = radius * reach / big_r;
            let d = (dir + east * (e * step) + north * (n * step)).normalize();
            let hh = surface_radius(planet, d) - sea;
            lo = lo.min(hh);
            hi = hi.max(hh);
            // Downhill is where the ground is LOWER than the middle, and
            // a sample further out weighs less per metre of fall because
            // it is measuring a gentler average.
            down += DVec2::new(e, n) * ((h - hh) / reach);
        }
    }
    Ground {
        low: lo,
        fall: hi - lo,
        down: down.normalize_or_zero(),
    }
}

/// How far over the sea a settlement may stand on a body, metres.
///
/// The ceiling is the PLANET's, not a fixed forty metres: on a world with
/// eight kilometres of relief a forty metre window is the coastal fringe
/// and nothing else, so every town came out on a beach and the interior
/// of every continent was empty. A city sits wherever the ground is
/// level, and level ground at two thousand metres is a plateau.
pub(crate) fn window(planet: &Planet) -> (f64, f64) {
    (3.0, (planet.relief * 0.3).max(40.0))
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
    let (low, high) = window(planet);
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
    // Where the water is, once for the body: a town's size is how far
    // it stands from it.
    let shore = Shore::of(planet, sea);
    let mut placed: Vec<Placement> = Vec::new();
    for (dir, h) in cands {
        if placed.len() >= count {
            break;
        }
        // Its size is its OWN, off how near the sea it stands, so the
        // great cities come out on the coast and the interior carries
        // market towns. It was the rank it happened to be accepted at.
        let radius = size_of(biggest, shore.distance(dir), planet, placed.len(), seed);
        // Apart by BOTH their radii, because they are not the same size:
        // one figure for the gap would stand a village as far off its
        // neighbour as a city stands off its own.
        if placed
            .iter()
            .any(|p| p.dir.dot(dir) > (((p.radius + radius) * OUTLINE + BETWEEN) / big_r).cos())
        {
            continue;
        }
        let Some(p) = settle(planet, sea, dir, h, radius) else {
            continue;
        };
        placed.push(p);
    }
    // The BIGGEST first, so the port is town nought and the index a town
    // carries is its rank. It is a sort rather than the acceptance order
    // because size is now a fact about the SITE and the order is a hash.
    placed.sort_by(|a, b| b.radius.total_cmp(&a.radius));
    lay_all(&placed, big_r, sea, seed)
}

/// A site that has been accepted: where it is, how big, how the ground
/// under it lies, and which way it grows.
pub(crate) struct Placement {
    pub dir: DVec3,
    /// The town's level, metres over the SEA, which is the lowest the
    /// natural ground reaches across the site.
    pub over_sea: f64,
    pub radius: f64,
    pub along: DVec2,
}

/// Accept a site if the ground across it is level enough, and answer
/// where its town would stand and which way it would grow.
///
/// **The level is the LOWEST ground across the site and never the height
/// at its middle.** A site blends the relief TOWARD its own height
/// (`Planet::surface_blend`), so a level taken at the middle of a sloping
/// site raises the downhill half: the town then stands on a pedestal with
/// its own skirt hanging over the land, which is a city that ADDED
/// ground rather than one that flattened it. Cut to the lowest and the
/// site can only ever take ground away, which is what grading is.
pub(crate) fn settle(
    planet: &Planet,
    sea: f64,
    dir: DVec3,
    h: f64,
    radius: f64,
) -> Option<Placement> {
    let ground = site_ground(planet, sea, dir, h, radius);
    // The bound is a SLOPE across whatever the survey walked, so
    // widening the rings to the town's own outline asks for the same
    // steepness over more ground rather than silently asking for a
    // flatter world. `LEVEL` was a fall over the old 1.05 radii.
    if ground.fall > (radius * OUTLINE * (LEVEL / 1.05)).min(CUT) {
        return None;
    }
    // And the level it will actually STAND at has to be inside the
    // window too. `plan` tests the candidate's own direction and this
    // takes the LOWEST of forty nine marches round it, which is a
    // different number by up to the `LEVEL` fall the site just passed:
    // a candidate accepted at the window's floor could settle under the
    // sea, and what would be built there is a levelled plateau with
    // water over it. The window is asked here rather than passed in
    // because `plan` and `road::waysides` both call this and a floor
    // handed in twice is a floor one caller gets wrong.
    let (low, _) = window(planet);
    if ground.low < low {
        return None;
    }
    // A town grows ALONG the shore, which is across the way the land
    // falls: the sea is downhill and the hill is up, so what is left to
    // build on runs between them.
    let along = DVec2::new(-ground.down.y, ground.down.x);
    Some(Placement {
        dir,
        over_sea: ground.low,
        radius,
        along,
    })
}

/// The towns of a list of placements, in the order given.
pub(crate) fn lay_all(placed: &[Placement], big_r: f64, sea: f64, seed: u32) -> Vec<Town> {
    lay_all_from(placed, big_r, sea, seed, 0)
}

/// The same, with the first one's index given: a settlement's index is
/// its own seed, so the villages a road grows carry on from the cities
/// rather than starting again at nought and being their twins.
pub(crate) fn lay_all_from(
    placed: &[Placement],
    big_r: f64,
    sea: f64,
    seed: u32,
    first: usize,
) -> Vec<Town> {
    placed
        .iter()
        .enumerate()
        .map(|(i, p)| {
            lay(
                p.dir,
                p.over_sea + sea - big_r,
                p.radius,
                p.along,
                first + i,
                seed,
            )
        })
        .collect()
}

/// A town's own seed, off the body's and its index: one function, so
/// anything that has to ask a town's plan the same question it asked
/// itself gets the same dice.
pub(crate) fn town_seed(seed: u32, index: usize) -> u32 {
    seed.wrapping_add(index as u32 * 977)
}

/// A town on a local grid: towers in the middle, streets of houses round
/// them, suburbs with gardens on the outside, and an outline that is not
/// a circle.
pub fn lay(dir: DVec3, h: f64, radius: f64, along: DVec2, index: usize, seed: u32) -> Town {
    let (east, north) = frame_at(dir);
    // The grid reaches past the nominal radius, because the lobes do, and
    // further still along the shore, because the town is stretched that
    // way.
    let n = ((radius * OUTLINE) / PITCH).ceil() as i64;
    let seed = town_seed(seed, index);
    let Plan { lots, pieces } = plot(n, radius, along, seed);
    Town {
        dir,
        east,
        north,
        h,
        radius,
        along,
        lots,
        pieces,
        index,
        seed,
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

/// How big a town is, how much town there is at a point of it, and
/// which zone that point is in.
mod shape;
pub use shape::*;

/// Which block carries what, and the lots on each.
mod plot;
use plot::Plan;
pub use plot::*;

/// The box counting dimension of a town's own plan, which is a
/// measurement and not a feature.
mod fractal;

/// How far a place stands from the sea, which is what sizes it.
mod shore;
pub use shore::*;

mod site;
pub use site::*;

mod street;
pub use street::*;
use street::{home_run, streets_of};

#[cfg(test)]
mod tests;
