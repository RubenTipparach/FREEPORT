//! The world this harness builds: the planet, its towns and everything in
//! them, and what a walker stands on.
//!
//! One `World`, built once at startup and shared behind an `Arc` so a
//! worker mid job keeps the world it had. `field_in` is what the mesher
//! asks, `underfoot` is what the walker asks, and the difference between
//! them is the whole of what the two ways of drawing this planet share.

use crate::terrain;
use crate::{Args, FINE, HEX_TILE, LUMPS, RADIUS, RELIEF, SEA, SEED, TOWNS, TOWN_RADIUS};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::columns::Columns;
use freeport_core::field::{Block, Built, Density, Planet, Structure, STREET};
use freeport_core::hex;
use freeport_core::recipe::{Building, Recipe};
use freeport_core::stack::Stacks;
use freeport_core::town::{self, lot_frame, Town};
use freeport_core::walker::Bounds;
use freeport_core::water::{Sea, Water};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// A town's structures, contiguous in `World::structures`, and the box
/// round all of them, so a chunk far from every town tests one box a town
/// and never a structure. Edits are pushed after the last group and are
/// tested one by one.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub lo: DVec3,
    pub hi: DVec3,
    pub range: std::ops::Range<usize>,
}

/// The field the harness stands on: the planet, what is built on it, and
/// where the walker may look for the ground. Shared with the workers, and
/// cloned whole for an edit, so a worker mid job keeps the world it had.
#[derive(Clone)]
pub(crate) struct World {
    pub planet: Planet,
    pub blocks: Vec<Block>,
    /// Every building and every piece of street on the planet.
    pub structures: Vec<Structure>,
    /// The structures a town at a time, with a box round each town's.
    pub groups: Vec<Group>,
    /// Every lamp in them, in the world frame, with its reach.
    pub lamps: Vec<(DVec3, f64)>,
    pub towns: Vec<Town>,
    pub bounds: Bounds,
    pub sea: Sea,
    /// Cuts made in the dry, which the sea never enters.
    pub dry: Vec<Block>,
    /// The hex world's grid, where the hex world is what is drawn: what
    /// the walker stands on is a column per tile and not the smooth field
    /// the columns are displaced by.
    pub tiles: Option<hex::Grid>,
    /// What has been built on those tiles, which the walker walks and the
    /// shader draws off one store.
    pub stacks: Stacks,
}

/// The field under the feet: the dual contoured world's, or the hex
/// world's columns. ONE type, so the walker is called from one place and
/// the two worlds cannot grow two collision rules between them.
pub(crate) enum Underfoot<'a> {
    Chunks(Built<'a>),
    Columns(Columns<'a>),
}

impl Density for Underfoot<'_> {
    fn at(&self, p: DVec3) -> f64 {
        match self {
            Underfoot::Chunks(f) => f.at(p),
            Underfoot::Columns(f) => f.at(p),
        }
    }

    fn material(&self, p: DVec3) -> u8 {
        match self {
            Underfoot::Chunks(f) => f.material(p),
            Underfoot::Columns(f) => f.material(p),
        }
    }
}

impl World {
    /// What a walker within `reach` of `p` stands on, which is the hex
    /// world's columns where the hex world is drawn and the dual contoured
    /// field where it is not.
    pub fn underfoot(&self, p: DVec3, reach: f64) -> Underfoot<'_> {
        match self.tiles {
            Some(grid) => Underfoot::Columns(Columns::new(grid, &self.planet, &self.stacks)),
            None => Underfoot::Chunks(self.field_near(p, reach)),
        }
    }

    /// The field inside a box, contoured on `cell`: the ground and every
    /// structure reaching into the box.
    pub fn field_in(&self, lo: DVec3, hi: DVec3, cell: f64) -> Built<'_> {
        let mut structures = Vec::new();
        let mut grouped = 0;
        for g in &self.groups {
            grouped = grouped.max(g.range.end);
            if g.lo.cmple(hi).all() && g.hi.cmpge(lo).all() {
                let town = &self.structures[g.range.clone()];
                structures.extend(town.iter().filter(|st| st.meets(lo, hi)));
            }
        }
        let loose = &self.structures[grouped.min(self.structures.len())..];
        structures.extend(loose.iter().filter(|st| st.meets(lo, hi)));
        Built {
            ground: &self.planet,
            blocks: self.blocks.clone(),
            structures,
            cell,
        }
    }

    /// The field within `reach` of a point, at full detail: what a walker
    /// stands on.
    pub fn field_near(&self, p: DVec3, reach: f64) -> Built<'_> {
        self.field_in(p - DVec3::splat(reach), p + DVec3::splat(reach), FINE)
    }

    /// The sea on that ground.
    pub fn water<'a>(&'a self, ground: &'a dyn Density) -> Water<'a> {
        Water {
            sea: self.sea,
            ground,
            dry: self.dry.clone(),
        }
    }
}

#[derive(Resource)]
pub(crate) struct Ground(pub Arc<World>);

/// The recipes in `assets/buildings`, by name.
fn recipes() -> HashMap<String, Recipe> {
    let mut out = HashMap::new();
    let Some(dir) = terrain::assets_dir().map(|d| d.join("buildings")) else {
        warn!("no assets folder: no recipes, so no buildings");
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        warn!("no recipes in {}", dir.display());
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Recipe::parse(&t))
            {
                Ok(r) => {
                    out.insert(r.name.clone(), r);
                }
                Err(e) => warn!("{}: {e}", path.display()),
            }
        }
    }
    out
}

/// Build it: the planet, its towns and everything in them. The hex world has
/// NO towns, because it draws the relief alone: `field.wgsl` has no sites
/// in it yet, so a town's levelled plateau would be in the walker's field
/// and not in the picture. What it has instead is the grid, so the walker
/// stands on a column.
pub(crate) fn build(args: &Args) -> World {
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
    let wanted = if args.tiers { 0 } else { TOWNS };
    let towns = town::plan(&planet, SEA, TOWN_RADIUS, wanted, SEED);
    planet.sites = towns.iter().map(town::site_of).collect();
    let planned = t0.elapsed();
    let recipes = recipes();
    let mut structures: Vec<Structure> = Vec::new();
    let mut groups = Vec::new();
    let mut buildings = 0;
    for t in &towns {
        let start = structures.len();
        for lot in &t.lots {
            let Some(recipe) = recipes.get(lot.recipe).or_else(|| recipes.get("house")) else {
                continue;
            };
            let storeys = lot.storeys.clamp(recipe.storeys[0], recipe.storeys[1]);
            let building = recipe.compile(storeys, SEED ^ lot.id);
            structures.push(Structure::new(lot_frame(RADIUS, t, lot.x, lot.z), building));
            buildings += 1;
        }
        for piece in &t.pieces {
            let slab = Building::slab(
                DVec3::new(0.0, 0.0, -0.05),
                DVec3::new(piece.w, piece.d, 0.6),
                STREET,
                0.0,
            );
            structures.push(Structure::new(lot_frame(RADIUS, t, piece.x, piece.z), slab));
        }
        let town = &structures[start..];
        groups.push(Group {
            lo: town.iter().fold(DVec3::INFINITY, |lo, st| lo.min(st.lo)),
            hi: town
                .iter()
                .fold(DVec3::NEG_INFINITY, |hi, st| hi.max(st.hi)),
            range: start..structures.len(),
        });
    }
    let lamps: Vec<(DVec3, f64)> = structures.iter().flat_map(Structure::lamps).collect();
    if let (Some(port), Some(lot)) = (towns.first(), towns.first().and_then(|t| t.lots.first())) {
        if let Some(recipe) = recipes.get(lot.recipe) {
            let f = lot_frame(RADIUS, port, lot.x, lot.z);
            let outside = f.world(DVec3::new(
                recipe.door,
                -recipe.footprint[1] / 2.0 - 2.5,
                1.7,
            ));
            let inside = f.world(DVec3::new(recipe.door, 0.0, 1.7));
            info!(
                "the port's first lot is a {} of {} storeys; its door from {:.2} looking at {:.2}",
                lot.recipe, lot.storeys, outside, inside
            );
        }
    }
    info!(
        "{} towns planned in {:.0} ms, {} buildings from {} recipes and {} pieces of street built in {:.0} ms, {} lamps",
        towns.len(),
        planned.as_secs_f64() * 1000.0,
        buildings,
        recipes.len(),
        structures.len() - buildings,
        (t0.elapsed() - planned).as_secs_f64() * 1000.0,
        lamps.len()
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
        blocks: Vec::new(),
        structures,
        groups,
        lamps,
        towns,
        bounds,
        sea: Sea { radius: SEA },
        dry: Vec::new(),
        tiles: args.tiers.then(|| hex::Grid::for_tile(RADIUS, HEX_TILE)),
        stacks: Stacks::new(),
    }
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
