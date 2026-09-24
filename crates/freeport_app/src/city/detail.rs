//! What every tile of a built town is drawn and walked into AS, following
//! the eye: a solid block from far off, the library's own bakes nearer,
//! and the boxes and lamps of it where a body can reach them.
//!
//! Everything is built on a worker (`AsyncComputeTaskPool`) and swapped in
//! when it lands, and the tile keeps whatever it was drawn as until then,
//! so a block is never a hole for a frame. What a tile is drawn as is its
//! own business and what it stops a body with is another, because a wall
//! is the same wall at every grade and only the tiles near a body need
//! one at all: the port is 153,000 boxes, and a city nine times its size
//! would be most of a gigabyte of them held for a view of the rooftops.

use super::district::{self, District};
use super::tiles::{self, Drawn, Stops, Tile, MASS};
use super::Glazing;
use crate::cull;
use crate::flight_bench::Urban;
use crate::stream::Frame as RenderFrame;
use crate::terrain::{Ground3d, TerrainMaterial};
use crate::tuning::Tuning;
use crate::world::{Fabric, Raised};
use crate::{Eye, Ground};
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::sync::Arc;

/// How near a tile has to be for its boxes and lamps to be built, and how
/// far off before they are dropped again, metres. Past what a car at its
/// own top speed covers in the few seconds a tile takes to come in, and
/// well past `lamps::REACH`, so a lamp is never missing for being in a
/// tile whose boxes have not been built.
const STOPS_NEAR: f64 = 180.0;
const STOPS_FAR: f64 = 260.0;

/// What one tile is drawn and walked as, now and on the way.
#[derive(Default)]
pub(crate) struct TileState {
    /// Its solid blocks, which it always has once the town is raised,
    /// and how many triangles they are.
    pub mass: Option<Entity>,
    pub mass_tris: usize,
    /// What it is drawn as nearer than that, and the grade, and how many
    /// triangles that is.
    pub shown: Option<(usize, Vec<Entity>)>,
    pub shown_tris: usize,
    /// The grade its distance asks for.
    pub grade: usize,
    /// A grade being built for it.
    pub job: Option<(usize, Task<Drawn>)>,
    /// Its boxes and lamps, where a body can reach it.
    pub stops: Option<Stops>,
    pub stops_job: Option<Task<Stops>>,
}

impl TileState {
    /// A tile freshly raised, drawn as its block.
    pub fn massed(mass: Option<Entity>, mass_tris: usize) -> Self {
        Self {
            mass,
            mass_tris,
            grade: MASS,
            ..default()
        }
    }

    /// How many triangles it is drawn with now.
    fn triangles(&self) -> usize {
        if self.shown.is_some() {
            self.shown_tris
        } else {
            self.mass_tris
        }
    }

    /// The grade it is drawn at now.
    fn drawn(&self) -> usize {
        self.shown.as_ref().map_or(MASS, |s| s.0)
    }
}

/// What a tile is built WITH: the library, the two materials and the
/// mesh store. One thing, because a system that draws a tile is not a
/// system with nine arguments.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Kit<'w> {
    pub library: Res<'w, super::stream::Library>,
    pub glass: Res<'w, Glazing>,
    pub material: Res<'w, Ground3d>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub tuning: Res<'w, Tuning>,
    pub proxy: Res<'w, cull::Proxy>,
}

/// One frame of it: land what has been built, work out what every tile
/// wants, and start the nearest of what is missing.
pub fn stream_tiles(
    mut commands: Commands,
    eye: Res<Eye>,
    ground: Res<Ground>,
    mut kit: Kit,
    mut fabric: ResMut<Fabric>,
) {
    let at = eye.0 .0 - ground.1;
    let world = ground.0.clone();
    let edges = [
        kit.tuning.building_lod_detail,
        kit.tuning.building_lod_near,
        kit.tuning.building_lod_far,
    ];
    let margin = kit.tuning.lod_hysteresis;
    // Bookkeeping moves every frame and is nobody's business; what a lamp
    // or a body is looking for is the boxes, and those say so themselves.
    let fabric_ref = fabric.bypass_change_detection();
    let mut stops_moved = false;
    let mut wants: Vec<(f64, usize, usize, Work)> = Vec::new();
    let mut busy = 0;
    let mut pending = 0;
    let mut drawn = Drawing::default();
    for (t, raised) in fabric_ref.towns.iter_mut().enumerate() {
        let here = raised.frame.local(at);
        for (k, state) in raised.state.iter_mut().enumerate() {
            stops_moved |= land(&mut commands, &mut kit, raised.entity, state);
            let d = raised.tiles[k].distance(here);
            state.grade = tiles::grade(state.grade, d, edges, margin);
            if state.grade == MASS && state.shown.is_some() {
                show(&mut commands, &kit, state, None, true);
            }
            if d > STOPS_FAR && state.stops.is_some() {
                state.stops = None;
                stops_moved = true;
            }
            busy += state.job.is_some() as usize + state.stops_job.is_some() as usize;
            drawn.tiles[state.drawn()] += 1;
            drawn.triangles += state.triangles();
            drawn.boxes += state.stops.as_ref().map_or(0, |s| s.blocks.len());
            let stops_wanted = d < STOPS_NEAR && state.stops.is_none();
            let draw_wanted = state.grade < MASS && state.drawn() != state.grade;
            if stops_wanted || draw_wanted || state.job.is_some() || state.stops_job.is_some() {
                pending += 1;
            }
            if stops_wanted && state.stops_job.is_none() {
                wants.push((d, t, k, Work::Stops));
            }
            if draw_wanted && state.job.as_ref().is_none_or(|j| j.0 != state.grade) {
                wants.push((d, t, k, Work::Draw(state.grade)));
            }
        }
        drawn.districts += district::follow(&mut commands, &mut raised.districts, &raised.state);
    }
    fabric_ref.tiles_pending = pending;
    fabric_ref.drawing = drawn;
    // Boxes before pictures, because a body walking into a wall that is
    // not there yet is worse than a block that is still a block; then the
    // nearest first.
    wants.sort_by(|a, b| {
        let first = |w: &Work| !matches!(w, Work::Stops);
        (first(&a.3), a.0)
            .partial_cmp(&(first(&b.3), b.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let budget = kit.tuning.building_jobs.saturating_sub(busy);
    for &(_, t, k, work) in wants.iter().take(budget) {
        start(&world, &kit.library.0, &mut fabric_ref.towns[t], k, work);
    }
    if stops_moved {
        fabric.set_changed();
    }
}

/// Put one tile's work on a worker: its boxes and lamps, or a grade of
/// its drawing.
fn start(
    world: &Arc<crate::world::World>,
    library: &Arc<crate::buildings::Library>,
    raised: &mut Raised,
    k: usize,
    work: Work,
) {
    let pool = AsyncComputeTaskPool::get();
    let (world, library, tiles) = (world.clone(), library.clone(), raised.tiles.clone());
    let town = raised.town;
    let state = &mut raised.state[k];
    match work {
        Work::Stops => {
            state.stops_job = Some(pool.spawn(async move {
                let t = &world.towns[town];
                tiles::stops(&library, t, &tiles[k], crate::RADIUS, crate::SEED)
            }));
        }
        Work::Draw(g) => {
            let sea = world.sea.radius;
            state.job = Some((
                g,
                pool.spawn(async move {
                    let t = &world.towns[town];
                    tiles::draw(&library, t, &tiles[k], g, crate::RADIUS, sea, crate::SEED)
                }),
            ));
        }
    }
}

/// What the built towns are drawn with this frame: how many tiles at each
/// grade, how many triangles all of them are, and how many boxes the
/// tiles a body can reach carry. What a picture's own metrics say.
#[derive(Default, Clone, Copy, Debug, serde::Serialize)]
pub struct Drawing {
    pub tiles: [usize; MASS + 1],
    /// How many districts are drawn whole, as one mesh for all their
    /// tiles' blocks.
    pub districts: usize,
    pub triangles: usize,
    pub boxes: usize,
}

/// Which of the two things a tile can be waiting on a worker for.
#[derive(Clone, Copy)]
enum Work {
    Stops,
    Draw(usize),
}

/// Take whatever a tile's workers have finished: its boxes, and a grade
/// it is shown at from now on. Answers whether its boxes moved.
fn land(commands: &mut Commands, kit: &mut Kit, parent: Entity, state: &mut TileState) -> bool {
    let mut moved = false;
    if let Some(done) = state.stops_job.as_mut().and_then(check_ready) {
        state.stops_job = None;
        state.stops = Some(done);
        moved = true;
    }
    if let Some((g, task)) = state.job.as_mut() {
        if let Some(drawn) = check_ready(task) {
            let g = *g;
            state.job = None;
            // A tile the eye has since left behind altogether keeps its
            // block: what was built for it is thrown away.
            if state.grade < MASS {
                let tris = drawn.triangles;
                let casts = cull::casts(g, &kit.tuning);
                let entities = spawn(commands, kit, parent, drawn, casts);
                show(commands, kit, state, Some((g, entities)), casts);
                state.shown_tris = tris;
            }
        }
    }
    moved
}

/// Draw a tile as a grade, or as its block with `None`, and take down what
/// it was drawn as before. A grade that does not cast its own shadow
/// leaves its block standing in `ShadowOnly`, which the sun draws and
/// the camera does not.
fn show(
    commands: &mut Commands,
    kit: &Kit,
    state: &mut TileState,
    now: Option<(usize, Vec<Entity>)>,
    casts: bool,
) {
    if let Some((_, old)) = state.shown.take() {
        for e in old {
            commands.entity(e).despawn();
        }
    }
    if let Some(mass) = state.mass {
        let mut block = commands.entity(mass);
        match (&now, casts) {
            (None, _) => block.remove::<MeshMaterial3d<cull::ShadowOnly>>().insert((
                Visibility::Inherited,
                MeshMaterial3d(kit.material.ground.clone()),
            )),
            (Some(_), false) => block
                .remove::<MeshMaterial3d<TerrainMaterial>>()
                .insert((Visibility::Inherited, MeshMaterial3d(kit.proxy.0.clone()))),
            (Some(_), true) => block.insert(Visibility::Hidden),
        };
    }
    state.shown = now;
}

/// A drawn tile's meshes as entities under its town: the opaque half on
/// the ground's own material, casting its own shadow or not, and the
/// glazing on the glass, which never does.
pub(crate) fn spawn(
    commands: &mut Commands,
    kit: &mut Kit,
    parent: Entity,
    drawn: Drawn,
    casts: bool,
) -> Vec<Entity> {
    let Some(aabb) = drawn.aabb else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(mesh) = drawn.opaque {
        let urban = Urban(triangles(&mesh));
        let mut e = commands.spawn((
            Mesh3d(kit.meshes.add(mesh)),
            MeshMaterial3d(kit.material.ground.clone()),
            Transform::default(),
            aabb,
            NoAutoAabb,
            urban,
            ChildOf(parent),
        ));
        if !casts {
            e.insert(bevy::light::NotShadowCaster);
        }
        out.push(e.id());
    }
    if let Some(mesh) = drawn.glass {
        let urban = Urban(triangles(&mesh));
        let e = commands
            .spawn((
                Mesh3d(kit.meshes.add(mesh)),
                MeshMaterial3d(kit.glass.0.clone()),
                bevy::light::NotShadowCaster,
                Transform::default(),
                aabb,
                NoAutoAabb,
                urban,
                ChildOf(parent),
            ))
            .id();
        out.push(e);
    }
    out
}

/// How many triangles a mesh is, which is what the benchmark sums.
fn triangles(mesh: &Mesh) -> usize {
    mesh.indices().map_or(mesh.count_vertices(), |i| i.len()) / 3
}

/// A town raised: its frame, its tiles and every tile's block, built on a
/// worker because a city of a thousand blocks is not a thing a frame does.
pub struct Raise {
    pub frame: freeport_core::town::Frame,
    pub tiles: Arc<Vec<Tile>>,
    pub blocks: Vec<Drawn>,
    /// Every district's tiles and all their blocks as one mesh.
    pub districts: Vec<(Vec<usize>, Drawn)>,
    pub bounds: (DVec3, DVec3),
}

/// Raise town `k` of a world off the frame: its tiles and their blocks.
pub fn raise(
    world: Arc<crate::world::World>,
    library: Arc<crate::buildings::Library>,
    k: usize,
) -> Task<Raise> {
    AsyncComputeTaskPool::get().spawn(async move {
        let town = &world.towns[k];
        let radius = crate::RADIUS;
        let frame = freeport_core::town::lot_frame(radius, town, 0.0, 0.0);
        let tiles = tiles::tiles_of(town, radius);
        let blocks: Vec<Drawn> = tiles
            .iter()
            .map(|t| {
                tiles::draw(
                    &library,
                    town,
                    t,
                    MASS,
                    radius,
                    world.sea.radius,
                    crate::SEED,
                )
            })
            .collect();
        // Every tile's plan bound, carried into the planet's frame and
        // widened by a street, which is what a body is asked about.
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for t in &tiles {
            for c in 0..8 {
                let pick = |bit: usize, a: f64, b: f64| if c & bit == 0 { a } else { b };
                let l = DVec3::new(
                    pick(1, t.lo.x, t.hi.x),
                    pick(2, t.lo.y, t.hi.y),
                    pick(4, t.lo.z, t.hi.z),
                );
                let p = frame.world(l);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        let pad = DVec3::splat(freeport_core::town::STREET);
        let sea = world.sea.radius;
        let districts = district::districts_of(&tiles)
            .into_iter()
            .map(|members| {
                let (lots, pieces) = district::parts_of(&tiles, &members);
                let part = (lots.as_slice(), pieces.as_slice());
                let drawn = tiles::draw_part(&library, town, part, MASS, radius, sea, crate::SEED);
                (members, drawn)
            })
            .collect();
        Raise {
            frame,
            tiles: Arc::new(tiles),
            blocks,
            districts,
            bounds: (lo - pad, hi + pad),
        }
    })
}

/// A raised town put in the world: the entity it is drawn under, placed
/// through the origin, and every tile's block under that.
pub fn stand(
    commands: &mut Commands,
    kit: &mut Kit,
    render: &RenderFrame,
    centre: DVec3,
    town: usize,
    raise: Raise,
) -> Raised {
    let frame = raise.frame;
    let at = freeport_core::pos::WorldPos(centre + frame.world(DVec3::ZERO));
    let basis = Mat3::from_cols(
        frame.east.as_vec3(),
        frame.north.as_vec3(),
        frame.dir.as_vec3(),
    );
    let parent = commands
        .spawn((
            Transform {
                translation: render.0.local(at),
                rotation: Quat::from_mat3(&basis),
                scale: Vec3::ONE,
            },
            Visibility::default(),
            crate::stream::Anchored { at },
        ))
        .id();
    let state = raise
        .blocks
        .into_iter()
        .map(|drawn| {
            let tris = drawn.triangles;
            let mass = spawn(commands, kit, parent, drawn, true).into_iter().next();
            TileState::massed(mass, tris)
        })
        .collect();
    // Every district starts HIDDEN over tiles that are all blocks: the
    // first frame of `stream_tiles` finds it whole and swaps it in for
    // them, in one frame, before anything is drawn.
    let districts = raise
        .districts
        .into_iter()
        .map(|(tiles, drawn)| {
            let entity = spawn(commands, kit, parent, drawn, true).into_iter().next();
            if let Some(e) = entity {
                commands.entity(e).insert(Visibility::Hidden);
            }
            District {
                tiles,
                entity,
                whole: false,
            }
        })
        .collect();
    Raised {
        town,
        frame,
        tiles: raise.tiles,
        state,
        districts,
        bounds: raise.bounds,
        entity: parent,
    }
}
