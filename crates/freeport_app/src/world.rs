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
/// One BUILT stretch of road: which it is and the lamps on it. It
/// carries no boxes, because tarmac stops nothing and a lamp post only
/// draws, and no entity, because the entity carries `roads::Paved` and
/// the query IS the record of what is standing.
pub(crate) struct Verge {
    pub which: (usize, usize),
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
}

impl Fabric {
    /// The field within `reach` of a point: the ground and the boxes of
    /// whatever is built near it, which is what makes a wall a wall to
    /// the body that meets it.
    pub fn underfoot<'a>(&'a self, planet: &'a Planet, p: DVec3, reach: f64) -> Built<'a> {
        let (lo, hi) = (p - DVec3::splat(reach), p + DVec3::splat(reach));
        let mut blocks = Vec::new();
        for t in &self.towns {
            if !(t.bounds.0.cmple(hi).all() && t.bounds.1.cmpge(lo).all()) {
                continue;
            }
            for b in &t.blocks {
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
    let mut routes = Vec::with_capacity(roads.len());
    for (road, run) in roads.iter().zip(&runs) {
        sites.extend(road::corridor(road, run, planet.radius, &discs));
        let line = road::centreline(road, planet.radius);
        let open = road::open(&line, planet.radius, &discs);
        let lit = road::lit(&line, planet.radius, &discs);
        let route = Route {
            line,
            run: run.clone(),
            open,
            lit,
            slip: (0, 0),
            sea: SEA,
        };
        routes.push(route);
    }
    let corridors = sites.len() - towns.len();
    planet.sites = sites.into();
    // And the SLIPS, one at each end of every road, which is what turns
    // a highway that STOPS near a town into one that joins its streets.
    //
    // AFTER the sites are installed, and that ordering is the whole of
    // it. A slip reads `town::surface_radius` to stand on the ground,
    // and the ground is the field WITH the towns' plateaus and the
    // roads' corridors cut into it: read off a planet whose `sites` are
    // still empty it stands on the BARE relief instead, which near a
    // town's mouth is metres under the corridor's own embankment. The
    // first picture of one showed the slip's own SHADOW curving across
    // an empty field with the tarmac nowhere in it, which is a road
    // buried under the ground it was laid on.
    for (route, road) in routes.iter_mut().zip(&roads) {
        for town_of in [road.from, road.to] {
            if let Some(town) = towns.get(town_of) {
                splice_slip(route, &planet, town, planet.radius);
            }
            route.flip();
        }
    }
    let planned = t0.elapsed();
    say_port(&towns);
    info!(
        "{} towns {} in {:.0} ms with {} roads cut into {} levelled corridor pieces; the nearest {} to the eye are BUILT and follow it",
        towns.len(),
        if baked.is_some() { "read" } else { "planned" },
        planned.as_secs_f64() * 1000.0,
        roads.len(),
        corridors,
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
    let f = lot_frame(RADIUS, port, lot.x, lot.z);
    let outside = f.world(DVec3::new(0.0, -town::BLOCK / 2.0 - 2.5, 1.7));
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
    /// The same road walked the other way, so one head splice serves
    /// both ends.
    fn flip(&mut self) {
        self.line.reverse();
        self.run.reverse();
        self.open.reverse();
        self.lit.reverse();
        self.slip = (self.slip.1, self.slip.0);
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
    // of its own; `open` is what `road::corridor` reads AND what the
    // ribbon lays tarmac on, so the slip is marked open to be DRAWN
    // while the corridor list was built before this splice and never
    // sees it. One flag doing two jobs is the thing to watch here, and
    // it is safe only because the sites are already fixed by now.
    route.open = std::iter::repeat_n(true, head)
        .chain(route.open[rest..].iter().copied())
        .collect();
    route.lit = std::iter::repeat_n(lit, head)
        .chain(route.lit[rest..].iter().copied())
        .collect();
    route.slip.0 = head;
}
