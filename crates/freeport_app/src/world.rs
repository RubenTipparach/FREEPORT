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
use freeport_core::town::{self, lot_frame, Frame, Town};
use freeport_core::walker::Bounds;
use freeport_core::water::{Sea, Water};
use std::sync::Arc;
use std::time::Instant;

/// A town's boxes, contiguous in `World::blocks`, and the box round all of
/// them, so a walker far from every town tests one box a town and never a
/// building.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub lo: DVec3,
    pub hi: DVec3,
    pub range: std::ops::Range<usize>,
}

/// The field the harness stands on: the planet, what is built on it, and
/// where the walker may look for the ground.
#[derive(Clone)]
pub(crate) struct World {
    pub planet: Planet,
    /// Every box every model was drawn from, a town at a time.
    pub blocks: Vec<Block>,
    pub groups: Vec<Group>,
    /// Every lamp in them, in the world frame, with its reach.
    pub lamps: Vec<(DVec3, f64)>,
    pub towns: Vec<Town>,
    pub bounds: Bounds,
    pub sea: Sea,
}

/// A town's geometry, ready to draw: one mesh in the town's own frame and
/// the frame it stands in. It is handed to the app at startup and spawned
/// once, so the workers never carry it.
pub(crate) struct TownMesh {
    pub frame: Frame,
    pub meshes: [DcMesh; 3],
}

impl World {
    /// What a walker within `reach` of `p` stands on: the ground, and the
    /// boxes of whatever is built near enough to be met.
    pub fn underfoot(&self, p: DVec3, reach: f64) -> Built<'_> {
        self.field_near(p, reach)
    }

    /// What a CHUNK is contoured on: the planet alone. A building is a
    /// model and not a brush, so nothing built is in the field the mesher
    /// sees, and a chunk under a city costs exactly what a chunk in the
    /// wilderness does.
    pub fn ground(&self) -> Built<'_> {
        Built::bare(&self.planet)
    }

    /// The field within `reach` of a point: the ground and the boxes near
    /// it, which is what makes a wall a wall to the body that meets it.
    pub fn field_near(&self, p: DVec3, reach: f64) -> Built<'_> {
        let (lo, hi) = (p - DVec3::splat(reach), p + DVec3::splat(reach));
        let mut blocks = Vec::new();
        for g in &self.groups {
            if !(g.lo.cmple(hi).all() && g.hi.cmpge(lo).all()) {
                continue;
            }
            for b in &self.blocks[g.range.clone()] {
                let (blo, bhi) = b.bounds();
                if blo.cmple(hi).all() && bhi.cmpge(lo).all() {
                    blocks.push(b);
                }
            }
        }
        Built {
            ground: &self.planet,
            blocks,
        }
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

/// Build it: the planet, its towns, and the models standing in them.
pub(crate) fn build(args: &Args) -> (World, Vec<TownMesh>) {
    let t0 = Instant::now();
    let mut planet = Planet {
        radius: RADIUS,
        relief: RELIEF,
        lumps: LUMPS,
        octaves: args.octaves,
        overhang: 3.0,
        ledge: 12.0,
        seed: SEED,
        sites: vec![],
    };
    let towns = town::plan(&planet, SEA, TOWN_RADIUS, TOWNS, SEED);
    planet.sites = towns.iter().map(town::site_of).collect();
    let planned = t0.elapsed();
    let raised = raise(&towns);
    say_port(&towns);
    info!(
        "{} towns planned in {:.0} ms, {} buildings and {} pieces of street modelled in {:.0} ms: {} triangles, {} boxes, {} lamps",
        towns.len(),
        planned.as_secs_f64() * 1000.0,
        raised.buildings,
        raised.pieces,
        (t0.elapsed() - planned).as_secs_f64() * 1000.0,
        raised.meshes.iter().map(|m| m.meshes[0].triangles()).sum::<usize>(),
        raised.blocks.len(),
        raised.lamps.len()
    );
    let (floor, roof) = planet.band();
    let bounds = Bounds {
        radius: RADIUS,
        floor: floor - 2.0,
        top: roof + 40.0,
        sea: 0.0,
    };
    (
        World {
            planet,
            blocks: raised.blocks,
            groups: raised.groups,
            lamps: raised.lamps,
            towns,
            bounds,
            sea: Sea { radius: SEA },
        },
        raised.meshes,
    )
}

/// What the towns' models come to: one mesh a town in the town's own
/// frame, every box and lamp in the world frame, and what they were built
/// from.
#[derive(Default)]
struct Raised {
    blocks: Vec<Block>,
    groups: Vec<Group>,
    lamps: Vec<(DVec3, f64)>,
    meshes: Vec<TownMesh>,
    buildings: usize,
    pieces: usize,
}

/// Every town modelled.
fn raise(towns: &[Town]) -> Raised {
    let mut out = Raised::default();
    let library = crate::buildings::Library::load();
    for town in towns {
        let f = model::fabric_with(town, RADIUS, |lot| library.model(lot, 0, SEED));
        let start = out.blocks.len();
        out.blocks.extend(f.blocks);
        let here = &out.blocks[start..];
        out.groups.push(Group {
            lo: here
                .iter()
                .fold(DVec3::INFINITY, |lo, b| lo.min(b.bounds().0)),
            hi: here
                .iter()
                .fold(DVec3::NEG_INFINITY, |hi, b| hi.max(b.bounds().1)),
            range: start..out.blocks.len(),
        });
        out.lamps.extend(f.lamps);
        out.buildings += f.buildings;
        out.pieces += f.pieces;
        out.meshes.push(TownMesh {
            frame: lot_frame(RADIUS, town, 0.0, 0.0),
            meshes: [
                f.mesh,
                model::fabric_with(town, RADIUS, |lot| library.model(lot, 1, SEED)).mesh,
                model::fabric_with(town, RADIUS, |lot| library.model(lot, 2, SEED)).mesh,
            ],
        });
    }
    out
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
