//! The world this harness builds: the planet, its towns and what a walker
//! stands on.
//!
//! One `World`, built once at startup and shared behind an `Arc` so a
//! worker mid job keeps the world it had. The ground is the PLANET and
//! nothing else, which is what a chunk is contoured on (`ground`), and
//! what stands on it is models (`freeport_core::model`), which the walker
//! meets as the boxes they were drawn from (`field_near`). That is the
//! whole of the difference between the two: one lattice carries terrain,
//! and a building is geometry beside it rather than a brush in it.

use crate::{Args, LUMPS, RADIUS, RELIEF, SEA, SEED, TOWNS, TOWN_RADIUS};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::dc::DcMesh;
use freeport_core::field::{Block, Built, Density, Planet};
use freeport_core::model;
use freeport_core::road::{self, Road};
use freeport_core::town::{self, lot_frame, Frame, Town};
use freeport_core::walker::Bounds;
use freeport_core::water::{Sea, Water};
use std::sync::Arc;
use std::time::Instant;

/// The field the harness stands on: the planet, what is built on it, and
/// where the walker may look for the ground.
#[derive(Clone)]
pub(crate) struct World {
    pub planet: Planet,
    pub towns: Vec<Town>,
    /// The roads joining them, as the lines the atlas holds.
    pub roads: Vec<Road>,
    /// What each of those roads is on the GROUND, in the same order.
    pub routes: Vec<Route>,
    pub bounds: Bounds,
    pub sea: Sea,
}

/// A road's own GROUND: its refined centreline, the level the corridor
/// was cut to under each point, and which of those points are outside
/// every town's own levelling.
///
/// One struct and not three vectors beside each other, because the three
/// are indexed together at every caller and a fourth would be the fourth
/// place to get an index wrong. The CENTRELINE is kept rather than
/// derived per stretch: it is a slerp off the atlas's waypoints and
/// 190,168 of them are 4.5 MB, against recomputing a whole road's worth
/// every time one stretch of it is laid.
#[derive(Clone, Debug, Default)]
pub(crate) struct Route {
    pub line: Vec<DVec3>,
    pub run: Vec<f64>,
    pub open: Vec<bool>,
    /// Which points this road's own CORRIDOR was cut at, which is
    /// `open` before the slip is spliced on: a slip stands on ground the
    /// TOWN levelled and has no embankment of its own to draw.
    pub graded: Vec<bool>,
    /// Which points are near enough a settlement to carry a lamp.
    pub lit: Vec<bool>,
    /// How many points at each END of the line are the SLIP that joins
    /// the highway to that town's own streets, head first.
    ///
    /// It is what lets the harness measure the junction and a camera
    /// frame it: without it the route's head is the crossing, so a walk
    /// in from there starts ON the paving and reports nought however
    /// much bare ground the road really ends in, which is the same
    /// tautology `road::clear` and `MEET` were measured through twice.
    pub slip: (usize, usize),
    /// The sea's radius, which is what a vertex's height is measured off
    /// for the shore band the ground shader reads.
    pub sea: f64,
    /// The GAS STATIONS along it, by the piece each stands on
    /// (`road::station::plan`), planned once the slips are spliced so
    /// the pieces they name are the pieces that are drawn.
    pub pumps: Vec<road::station::Station>,
    /// Which points stand on another road's TRUNK or fork off it through
    /// the owner's corridor (`road::trunk::merge`), so the census can
    /// tell a join from a mouth left bare.
    pub trunk: Vec<bool>,
    /// How many points at the head and the tail stand on another road's
    /// TRUNK: an end that is not this road's own gets no slip, because
    /// the owner's is there.
    pub shared: (usize, usize),
}

/// One BUILT town: the boxes its models were drawn from, the lamps in
/// them and the entity drawing it.
///
/// Per town rather than one flat list with ranges into it, because what
/// is built STREAMS: a town comes into range as you drive and another
/// leaves, and a range into a shared vector cannot be taken out of the
/// middle without moving every range after it.
pub(crate) struct Raised {
    /// Which of `World::towns` this is, which never moves.
    pub town: usize,
    pub blocks: Vec<Block>,
    pub bounds: (DVec3, DVec3),
    pub lamps: Vec<(DVec3, f64)>,
    pub entity: Entity,
}

/// What is BUILT on the world right now.
///
/// It is not part of `World` because `World` is behind an `Arc` the
/// mesher's workers hold: the PLANET never changes (every town on the
/// body levels its own ground from the first frame, whether or not
/// anybody has built it), and the buildings come and go with the eye.
/// One BUILT stretch of road: which it is, the lamps on it, and the
/// boxes of whatever stands on it, which is a gas station's pumps,
/// pillars and kiosk: tarmac stops nothing and a lamp post only draws,
/// but a pump is a thing a car pulls up to and a walker walks up to. No
/// entity, because the entity carries `roads::Paved` and the query IS
/// the record of what is standing.
pub(crate) struct Verge {
    pub which: (usize, usize),
    pub blocks: Vec<Block>,
    pub bounds: (DVec3, DVec3),
    /// Each lamp's own index ALONG THE ROAD, where it is and how far it
    /// throws. The index is along the road and not along the stretch,
    /// because a stretch streams and an index into one would name a
    /// different lamp the moment a neighbour arrived.
    pub lamps: Vec<(usize, DVec3, f64)>,
}

#[derive(Resource, Default)]
pub(crate) struct Fabric {
    pub towns: Vec<Raised>,
    /// The stretches of road standing, which carry lamps of their own on
    /// the approaches to a town.
    pub verges: Vec<Verge>,
}

/// The GROUND and what is BUILT on it: one thing, because what a body
/// stands on is one question and asking it as two arguments took three
/// systems over Bevy's own parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Surface<'w> {
    pub ground: Res<'w, Ground>,
    pub fabric: Res<'w, Fabric>,
    /// Which BODY that ground belongs to, because a crowd and a stolen
    /// car belong to one and asking this planet's questions of another
    /// planet's townsmen is the mistake `show_cars` already guards.
    ///
    /// OPTIONAL, because a `SystemParam` that demands a resource is a
    /// resource every harness that uses it has to insert: three of the
    /// fly camera's own tests build an app with a ground and no system
    /// of planets, and they had nothing to do with what this was added
    /// for. A world with no list of bodies is the HOME body, which is
    /// the one `turn_out` is handed.
    pub planets: Option<Res<'w, crate::planets::Planets>>,
}

impl Surface<'_> {
    /// What a body within `reach` of a point stands on.
    pub fn underfoot(&self, p: DVec3, reach: f64) -> Built<'_> {
        self.fabric.underfoot(&self.ground.0.planet, p, reach)
    }

    /// The world the ground belongs to.
    pub fn world(&self) -> &World {
        &self.ground.0
    }

    /// Where that world's centre is.
    pub fn centre(&self) -> DVec3 {
        self.ground.1
    }

    /// Which body is active, and the HOME body where there is no list.
    pub fn body(&self) -> usize {
        self.planets.as_ref().map_or(0, |p| p.active)
    }
}

impl Fabric {
    /// The field within `reach` of a point: the ground and the boxes of
    /// whatever is built near it, which is what makes a wall a wall to
    /// the body that meets it.
    pub fn underfoot<'a>(&'a self, planet: &'a Planet, p: DVec3, reach: f64) -> Built<'a> {
        let (lo, hi) = (p - DVec3::splat(reach), p + DVec3::splat(reach));
        let mut blocks = Vec::new();
        // The towns' walls and the roads' stations, which are the two
        // things standing on this world that stop a body.
        let built = self
            .towns
            .iter()
            .map(|t| (&t.bounds, &t.blocks))
            .chain(self.verges.iter().map(|v| (&v.bounds, &v.blocks)));
        for (bounds, held) in built {
            if !(bounds.0.cmple(hi).all() && bounds.1.cmpge(lo).all()) {
                continue;
            }
            for b in held {
                let (blo, bhi) = b.bounds();
                if blo.cmple(hi).all() && bhi.cmpge(lo).all() {
                    blocks.push(b);
                }
            }
        }
        Built {
            ground: planet,
            blocks,
        }
    }

    /// Which planned towns are built, sorted, so a wanted set and a
    /// standing set can be compared.
    pub fn standing(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self.towns.iter().map(|t| t.town).collect();
        out.sort_unstable();
        out
    }
}

/// A town's geometry, ready to draw: one mesh in the town's own frame and
/// the frame it stands in. It is handed to the app at startup and spawned
/// once, so the workers never carry it.
pub(crate) struct TownMesh {
    pub frame: Frame,
    pub meshes: [DcMesh; 3],
}

impl World {
    /// What a CHUNK is contoured on: the planet alone. A building is a
    /// model and not a brush, so nothing built is in the field the mesher
    /// sees, and a chunk under a city costs exactly what a chunk in the
    /// wilderness does.
    pub fn ground(&self) -> Built<'_> {
        Built::bare(&self.planet)
    }

    /// The sea on that ground.
    pub fn water<'a>(&'a self, ground: &'a dyn Density) -> Water<'a> {
        Water {
            sea: self.sea,
            ground,
        }
    }
}

#[derive(Resource)]
pub(crate) struct Ground(pub Arc<World>, pub DVec3);

/// The home body, with nothing levelled into it yet. One function, so the
/// bake and the game plan the SAME planet: an atlas of a body that is not
/// this one puts cities in the sea, and the only way to be sure it is the
/// same body is for there to be one place that says what the body is.
pub(crate) fn home_planet(octaves: u32) -> Planet {
    Planet {
        radius: RADIUS,
        relief: RELIEF,
        lumps: LUMPS,
        octaves,
        overhang: 3.0,
        ledge: 12.0,
        seed: SEED,
        sites: vec![].into(),
    }
}

/// The body the atlas is the plan of.
pub(crate) const HOME: &str = "Freeport";

/// Plan the home body and write its atlas out. This is the whole of
/// `--bake-atlas`, and it touches no window and no GPU.
pub(crate) fn bake_atlas(args: &Args) {
    let t0 = Instant::now();
    let planet = home_planet(args.octaves);
    let atlas = crate::atlas::Atlas::plan(HOME, &planet, SEA, TOWN_RADIUS, TOWNS);
    let Some(path) = crate::atlas::path_of(HOME) else {
        eprintln!("no assets folder to write an atlas into");
        return;
    };
    let roads = atlas.roads();
    let joined: std::collections::BTreeSet<usize> =
        roads.iter().flat_map(|r| [r.from, r.to]).collect();
    let metres: f64 = roads.iter().map(|r| r.length(planet.radius)).sum();
    match crate::atlas::write(&atlas, &path) {
        Ok(()) => println!(
            "{} planned in {:.1} s: {} towns, {} roads over {:.0} km joining {} of them, written to {}",
            HOME,
            t0.elapsed().as_secs_f64(),
            atlas.towns.len(),
            roads.len(),
            metres / 1000.0,
            joined.len(),
            path.display()
        ),
        Err(e) => eprintln!("atlas not written: {e}"),
    }
    say_shore(&planet, &atlas);
}

/// How far the body's land and its towns stand from the SEA, which is
/// the measurement `town::COAST` is set against: the median over the
/// land, and the towns by size, biggest quarter against smallest.
fn say_shore(planet: &Planet, atlas: &crate::atlas::Atlas) {
    let shore = town::Shore::of(planet, SEA);
    let bare = planet.bare();
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let mut land: Vec<f64> = (0..20_000)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / 20_000.0;
            let s = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f64;
            DVec3::new(s * a.cos(), y, s * a.sin())
        })
        .filter(|d| bare.surface(*d).0 + planet.radius >= SEA)
        .map(|d| shore.distance(d))
        .collect();
    land.sort_by(f64::total_cmp);
    let median = land.get(land.len() / 2).copied().unwrap_or(0.0);
    let mut towns: Vec<(f64, f64)> = atlas
        .towns
        .iter()
        .map(|t| (t.r, shore.distance(DVec3::from_array(t.dir))))
        .collect();
    towns.sort_by(|a, b| b.0.total_cmp(&a.0));
    let quarter = (towns.len() / 4).max(1);
    let mean = |q: &[(f64, f64)]| q.iter().map(|t| t.1).sum::<f64>() / q.len().max(1) as f64;
    println!(
        "the land stands a median {:.0} km from the sea; the biggest quarter of the {} settlements a mean {:.0} km from it and the smallest quarter {:.0} km, the port {:.0} km",
        median / 1000.0,
        towns.len(),
        mean(&towns[..quarter.min(towns.len())]) / 1000.0,
        mean(&towns[towns.len().saturating_sub(quarter)..]) / 1000.0,
        towns.first().map_or(0.0, |t| t.1) / 1000.0,
    );
}

/// One road's ROUTE before its slips and its stations are on it: the
/// centreline the atlas's waypoints fit to, the ground the corridor was
/// cut to, and which of its points carry tarmac and which a lamp.
fn route_of(road: &Road, run: &[f64], radius: f64, discs: &freeport_core::field::Sites) -> Route {
    let line = road::centreline(road, radius);
    let open = road::open(&line, radius, discs);
    let lit = road::lit(&line, radius, discs);
    Route {
        trunk: vec![false; line.len()],
        line,
        run: run.to_vec(),
        graded: open.clone(),
        open,
        lit,
        slip: (0, 0),
        sea: SEA,
        pumps: Vec::new(),
        shared: (0, 0),
    }
}

/// Every road's laying handed to `road::trunk::merge`, which moves a road
/// standing on another's trunk onto that road's line and profile.
fn merge_trunks(routes: &mut [Route], radius: f64) -> road::trunk::Merged {
    let mut lanes: Vec<road::trunk::Laying<'_>> = routes
        .iter_mut()
        .map(|r| road::trunk::Laying {
            line: &mut r.line,
            run: &mut r.run,
            open: &mut r.open,
            graded: &mut r.graded,
            lit: &mut r.lit,
            trunk: &mut r.trunk,
            shared: &mut r.shared,
        })
        .collect();
    road::trunk::merge(&mut lanes, radius)
}

/// Every road's centreline, profile and flags, MERGED where they share
/// a trunk, and the corridor under each one pushed onto `sites`.
///
/// The merge comes before anything is cut from the lines: every road
/// out of a town runs on the same chain of waypoints as its neighbours
/// until their routes split, and laid on their own they stood three and
/// four deep with the ground stepping between their profiles.
fn lay_routes(
    roads: &[Road],
    runs: &[Vec<f64>],
    radius: f64,
    discs: &freeport_core::field::Sites,
    sites: &mut Vec<freeport_core::town::Site>,
) -> Vec<Route> {
    let mut routes: Vec<Route> = roads
        .iter()
        .zip(runs)
        .map(|(road, run)| route_of(road, run, radius, discs))
        .collect();
    let merged = merge_trunks(&mut routes, radius);
    info!(
        "{} roads stand on another road's trunk for {} points, the longest trunk {:.1} km, the steepest piece at a shared point climbs at {:.1}%, and the profiles settled in {} passes",
        merged.roads,
        merged.points,
        merged.longest / 1000.0,
        merged.steepest * 100.0,
        merged.passes
    );
    for route in &routes {
        sites.extend(road::corridor_of(&route.line, &route.run, &route.open));
    }
    routes
}

/// The SLIPS, one at each end of every road, which is what turns a
/// highway that STOPS near a town into one that joins its streets.
///
/// AFTER the sites are installed, and that ordering is the whole of it.
/// A slip reads `town::surface_radius` to stand on the ground, and the
/// ground is the field WITH the towns' plateaus and the roads' corridors
/// cut into it: read off a planet whose `sites` are still empty it
/// stands on the BARE relief instead, which near a town's mouth is
/// metres under the corridor's own embankment. The first picture of one
/// showed the slip's own SHADOW curving across an empty field with the
/// tarmac nowhere in it, which is a road buried under the ground it was
/// laid on.
fn join_towns(routes: &mut [Route], roads: &[Road], towns: &[Town], planet: &Planet) {
    for (route, road) in routes.iter_mut().zip(roads) {
        for town_of in [road.from, road.to] {
            // An end standing on another road's trunk has that road's
            // slip and none of its own.
            if let Some(town) = towns.get(town_of).filter(|_| route.shared.0 == 0) {
                splice_slip(route, planet, town, planet.radius);
            }
            route.flip();
        }
    }
}

/// Build it: the planet, its towns, the roads between them, and the
/// models standing in them.
pub(crate) fn build(args: &Args) -> World {
    let t0 = Instant::now();
    let mut planet = home_planet(args.octaves);
    // The BAKED plan if there is one, which is the point of baking it:
    // twenty thousand candidate directions and a search over a hundred
    // thousand waypoints is seconds of work every launch for an answer
    // that never changes. Planning here is the fallback, so a checkout
    // nobody has baked still runs and says so.
    let baked = crate::atlas::load(HOME, &planet, SEA, TOWN_RADIUS);
    let (towns, roads, runs) = match &baked {
        Some(a) => (a.towns(), a.roads(), a.runs()),
        None => {
            warn!(
                "no atlas for {HOME}: planning it here, which takes seconds.                  `--bake-atlas` writes one and this becomes a file read."
            );
            let towns = town::plan(&planet, SEA, TOWN_RADIUS, TOWNS, SEED);
            (towns, Vec::new(), Vec::new())
        }
    };
    // The ground a town levels and the CORRIDOR a road is cut along, in
    // one list, because the field reads one list. Both are planet wide
    // from the first frame for the same reason: a chunk is meshed once,
    // so ground that a road will stand on has to be levelled before the
    // chunk over it is contoured, and `field::Sites` is the index that
    // makes a body carrying hundreds of thousands of them cost a chunk
    // what a body carrying eight towns did.
    // The towns' own discs first, as an INDEX, because the corridors are
    // cut against them: a road ends at a town and passes through the
    // villages it grew, and inside a town's levelling the ground is the
    // town's to hold and its streets are the town's to pave.
    let discs: freeport_core::field::Sites = towns.iter().map(town::site_of).collect();
    let mut sites: Vec<_> = discs.iter().copied().collect();
    let mut routes = lay_routes(&roads, &runs, planet.radius, &discs, &mut sites);
    let corridors = sites.len() - towns.len();
    planet.sites = sites.into();
    join_towns(&mut routes, &roads, &towns, &planet);
    // And the GAS STATIONS, after the slips, so the pieces they stand
    // on are the pieces that are drawn.
    let mut pumps = 0;
    for route in &mut routes {
        route.pumps = road::station::plan(route.course(), planet.radius);
        pumps += route.pumps.len();
    }
    say_pumps(&routes, planet.radius);
    let planned = t0.elapsed();
    say_port(&towns);
    info!(
        "{} towns {} in {:.0} ms with {} roads cut into {} levelled corridor pieces and {} gas stations; the nearest {} to the eye are BUILT and follow it",
        towns.len(),
        if baked.is_some() { "read" } else { "planned" },
        planned.as_secs_f64() * 1000.0,
        roads.len(),
        corridors,
        pumps,
        crate::TOWNS_BUILT,
    );
    let (floor, roof) = planet.band();
    let bounds = Bounds {
        radius: RADIUS,
        floor: floor - 2.0,
        top: roof + 40.0,
        sea: 0.0,
    };
    World {
        planet,
        towns,
        roads,
        routes,
        bounds,
        sea: Sea { radius: SEA },
    }
}

/// ONE town modelled: its three LODs of mesh in its own frame, and the
/// boxes and lamps it puts in the world.
///
/// A town at a time rather than the whole list, because what is built
/// STREAMS: raising one is what a frame can afford and raising all of
/// them is not.
pub(crate) struct Lifted {
    pub mesh: TownMesh,
    pub blocks: Vec<Block>,
    pub lamps: Vec<(DVec3, f64)>,
    pub buildings: usize,
    pub pieces: usize,
}

pub(crate) fn raise_one(library: &crate::buildings::Library, town: &Town) -> Lifted {
    let f = model::fabric_with(town, RADIUS, |lot| library.model(lot, 0, SEED));
    Lifted {
        mesh: TownMesh {
            frame: lot_frame(RADIUS, town, 0.0, 0.0),
            meshes: [
                f.mesh,
                model::fabric_with(town, RADIUS, |lot| library.model(lot, 1, SEED)).mesh,
                model::fabric_with(town, RADIUS, |lot| library.model(lot, 2, SEED)).mesh,
            ],
        },
        blocks: f.blocks,
        lamps: f.lamps,
        buildings: f.buildings,
        pieces: f.pieces,
    }
}

/// Where the first gas station out of the port stands, so a picture can
/// be aimed at it and a drive can be checked against it.
fn say_pumps(routes: &[Route], radius: f64) {
    let Some((r, route, pump)) = routes
        .iter()
        .enumerate()
        .find_map(|(r, route)| route.pumps.first().map(|p| (r, route, p)))
    else {
        return;
    };
    let Some(middle) = road::station::middle(route.course(), pump, radius) else {
        return;
    };
    info!(
        "the first gas station is on road {r} at piece {}, its forecourt at {:.0}, {:.1} km along the road",
        pump.piece,
        middle,
        (0..pump.piece)
            .map(|k| route.line[k].angle_between(route.line[k + 1]) * radius)
            .sum::<f64>()
            / 1000.0
    );
}

/// Where the port is, so a picture can be aimed at it: its middle, its
/// levelled ground and how far that reaches, and what its first lot
/// carries.
fn say_port(towns: &[Town]) {
    let Some(port) = towns.first() else {
        return;
    };
    let site = town::site_of(port);
    info!(
        "the port is at {:.0}, its ground {:.1} m over the mean radius, levelled over {:.0} m",
        port.dir * RADIUS,
        site.h,
        site.r,
    );
    let Some(lot) = port.lots.first() else {
        return;
    };
    let f = lot_frame(RADIUS, port, lot.x, lot.z).turned(lot.yaw);
    let outside = f.world(DVec3::new(0.0, -lot.w / 2.0 - 2.5, 1.7));
    let inside = f.world(DVec3::new(0.0, 0.0, 1.7));
    info!(
        "the port's first lot is a {} of {} storeys; its door from {:.2} looking at {:.2}",
        lot.kind.name(),
        lot.storeys,
        outside,
        inside
    );
}

/// Where a walker starts: on a street of the port, facing the middle of
/// town, or on the pole if there is no town.
pub(crate) fn start(world: &World) -> (DVec3, DVec3) {
    let Some(port) = world.towns.first() else {
        let top = town::surface_radius(&world.planet, DVec3::Y);
        return (DVec3::new(0.5, top, -6.5), DVec3::new(0.0, top + 0.8, 0.0));
    };
    let x = -town::BLOCK / 2.0 - town::STREET / 2.0;
    let f = lot_frame(RADIUS, port, x, -port.radius * 0.6);
    (
        f.world(DVec3::new(0.0, 0.0, 1.7)),
        f.world(DVec3::new(0.0, 10.0, 1.7)),
    )
}

/// The nearest point of the shore to the eye, on rings of directions out
/// from it, so a picture can be aimed at the sea. The rings are measured
/// in ANGLE out to a quarter turn and not in metres, because a length
/// picked on a ten kilometre planet reaches a hundredth of the way round
/// a thousand kilometre one: twenty five metres a ring found no sea at
/// all out there and the line printed NaN.
pub(crate) fn shore(world: &World, eye: DVec3) -> Option<DVec3> {
    let up = eye.normalize_or(DVec3::Y);
    let (east, north) = town::frame_at(up);
    for ring in 1..400 {
        let a = ring as f64 * std::f64::consts::FRAC_PI_2 / 400.0;
        for k in 0..(ring * 6) {
            let b = k as f64 / (ring * 6) as f64 * std::f64::consts::TAU;
            let dir = (up * a.cos() + (east * b.cos() + north * b.sin()) * a.sin()).normalize();
            if town::surface_radius(&world.planet, dir) < world.sea.radius {
                return Some(dir * world.sea.radius);
            }
        }
    }
    None
}

impl Route {
    /// The whole of it as the CORE's own view of a road
    /// (`ribbon::Course`), which is what the ribbon, the mound and the
    /// traffic on it all read.
    pub fn course(&self) -> freeport_core::road::ribbon::Course<'_> {
        self.stretch(0..self.line.len())
    }

    /// One span of it, the same way.
    pub fn stretch(&self, at: std::ops::Range<usize>) -> freeport_core::road::ribbon::Course<'_> {
        freeport_core::road::ribbon::Course {
            line: &self.line[at.clone()],
            run: &self.run[at.clone()],
            open: &self.open[at.clone()],
            graded: &self.graded[at.clone()],
            lit: &self.lit[at.clone()],
            pumps: &self.pumps,
            first: at.start,
        }
    }

    /// The same road walked the other way, so one head splice serves
    /// both ends.
    fn flip(&mut self) {
        self.line.reverse();
        self.run.reverse();
        self.open.reverse();
        self.graded.reverse();
        self.lit.reverse();
        self.trunk.reverse();
        self.slip = (self.slip.1, self.slip.0);
        self.shared = (self.shared.1, self.shared.0);
        // A station keeps its place on the ground: its piece counts from
        // the other end now and its side is the other side of a road
        // walked the other way.
        let n = self.line.len();
        for p in &mut self.pumps {
            p.piece = n.saturating_sub(2).saturating_sub(p.piece);
            p.side = -p.side;
        }
    }
}

/// Lay a SLIP from a road's own mouth into the town at its near end, and
/// splice it onto the head of the route so the road RUNS to a crossing
/// rather than stopping in a field short of one.
///
/// Spliced rather than drawn beside, because a route is what everything
/// downstream reads: the stretches that stream, the lamps that light
/// them and the `roads::Network` a car follows. A slip that was a mesh
/// of its own would be tarmac a car could not drive onto, which is the
/// same defect one level up.
fn splice_slip(route: &mut Route, planet: &Planet, town: &freeport_core::town::Town, radius: f64) {
    // The first point the corridor is actually CUT at, which is where
    // the tarmac starts and where the town's own levelling gives out.
    let Some(first) = route.open.iter().position(|o| *o) else {
        return;
    };
    if first + 1 >= route.line.len() {
        return;
    }
    let at = route.line[first];
    let at_h = route.run[first];
    // The road's own heading at the mouth, pointing IN toward the town,
    // so the slip leaves the highway straight rather than kinking off it.
    let along = (at - route.line[first + 1]).normalize_or(at);
    let slip = road::slip(planet, town, at, at_h, along, radius);
    if slip.len() < 2 {
        return;
    }
    // The slip runs mouth to crossing, and a route runs town OUT, so it
    // goes on the head the other way up.
    //
    // Its FIRST point IS the mouth, and the route already has that
    // point: grafted whole, the line carried the mouth TWICE, once at
    // the slip's own height and once at the highway's, a piece of no
    // length at all with `EMBANK` across it. Measured on the body, that
    // was the steepest road piece there is: **-0.50 m over 0.015 m, a
    // grade of 3355%**. The highway's own point is the one that stays,
    // because it is the one the baked profile and the corridor under it
    // agree about.
    let rest = first;
    let head = slip.len() - 1;
    let slip = &slip[1..];
    let lit = route.lit.get(first).copied().unwrap_or(false);
    route.line = slip
        .iter()
        .rev()
        .map(|(d, _)| *d)
        .chain(route.line[rest..].iter().copied())
        .collect();
    route.run = slip
        .iter()
        .rev()
        .map(|(_, h)| *h)
        .chain(route.run[rest..].iter().copied())
        .collect();
    // A slip is INSIDE the town's own levelling, so it cuts no corridor
    // of its own; `open` is what the ribbon lays TARMAC on, so the slip
    // is marked open to be drawn.
    route.open = std::iter::repeat_n(true, head)
        .chain(route.open[rest..].iter().copied())
        .collect();
    // And NOT graded: the corridor list was built before this splice and
    // never sees the slip, so the road has no embankment there and the
    // ribbon must not draw one. `open` and `graded` are the two jobs one
    // flag used to do.
    route.graded = std::iter::repeat_n(false, head)
        .chain(route.graded[rest..].iter().copied())
        .collect();
    route.lit = std::iter::repeat_n(lit, head)
        .chain(route.lit[rest..].iter().copied())
        .collect();
    route.trunk = std::iter::repeat_n(false, head)
        .chain(route.trunk[rest..].iter().copied())
        .collect();
    route.slip.0 = head;
}
