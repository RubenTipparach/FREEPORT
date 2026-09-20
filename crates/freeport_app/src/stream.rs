//! The chunk streamer: the chunks the rings want, contoured on workers, and
//! drawn through the floating origin.
//!
//! Every frame the rings follow the eye (`Rings::follow`) and, when a box
//! moves, a background planner recomputes the wanted set: every chunk of
//! every level's box
//! that the field cannot rule wholly rock or air, each with the signature
//! of its neighbours' levels, which is everything its mesh depends on
//! besides the field. A wanted chunk that is not loaded with that
//! signature is a job, nearest first and new before rebuilt, contoured on
//! a worker thread from the same field and the same rings, and drawn when
//! it comes back if it is still wanted as it was. A replacement layout is
//! uploaded hidden, then published together once all its seams are ready.
//! The previous layout remains visible until that swap. The target rings
//! stay fixed during a build, so moving cannot keep cancelling a transition.
//! Work is ordered once for that layout. Publication hides retired meshes in
//! the same flush that reveals their replacements; destruction is amortized.
//!
//! A chunk's mesh is `f32` metres from its own `f64` corner, and its
//! entity is placed from that corner through the origin (`pos::Origin`),
//! which follows the eye and moves every chunk with it when it does. The
//! subtraction is the precise step and it happens once, in `f64`.

use crate::compute::Sampler;
use crate::lod_debug::{LodDebug, TerrainLod};
use crate::meshing::{spawn_workers, Done, Job};
use crate::terrain::TerrainMaterial;
use crate::water::{recentre, Sheet, WaterMaterial};
use crate::{Eye, Ground, World};
use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::field::Density;
use freeport_core::lattice::{ChunkId, Lattice, Rings};
use freeport_core::pos::{Origin, WorldPos};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod planning;
use planning::{Layout, Planner, Request};

/// How long the eye's own pace is eased over, seconds. Short enough that
/// pulling away or braking is felt inside a box's own life and long
/// enough that one slow frame is not a speed.
const PACE_EASE: f64 = 0.75;

/// Anything DRAWN at a place in the world: a chunk, its sheet of sea, a
/// lamp, a town's models. It carries where it is in the world frame,
/// which is what the origin's move needs, and `rebase_origin` is the one
/// system that moves any of them.
#[derive(Component)]
pub struct Anchored {
    pub at: WorldPos,
}

/// The floating origin the render frame is measured from.
#[derive(Resource, Default)]
pub struct Frame(pub Origin);

/// What the streamer has done, for the status line and the log.
#[derive(Default, Clone)]
pub struct Stats {
    pub loaded: usize,
    pub pending: usize,
    pub triangles: usize,
    pub built: usize,
    /// Worker milliseconds over every chunk built, and over the last.
    pub work_ms: f32,
    pub last_ms: f32,
    pub wanted: usize,
}

type Loaded = (Vec<Entity>, u64, usize);

#[derive(Resource)]
pub struct Streamer {
    pub centre: DVec3,
    epoch: u64,
    pub lat: Lattice,
    pub rings: Rings,
    wanted: HashMap<ChunkId, u64>,
    /// What is drawn for a chunk: its entities (ground, and the sea's
    /// sheet where there is one), the signature it was built for, and its
    /// triangles.
    loaded: HashMap<ChunkId, Loaded>,
    staged: HashMap<ChunkId, Loaded>,
    /// Freeze a requested layout until it can replace the visible one.
    building: bool,
    has_layout: bool,
    planner: Planner,
    planning: bool,
    todo: VecDeque<(ChunkId, u64)>,
    remaining: usize,
    /// Hidden immediately at publication, destroyed under the upload budget.
    retired: VecDeque<Entity>,
    pending: HashMap<ChunkId, (u64, Arc<AtomicBool>)>,
    jobs: Sender<Job>,
    /// Behind a mutex only because a resource must be `Sync`; the main
    /// thread is the one reader.
    done: Mutex<Receiver<Done>>,
    workers: usize,
    tuning: crate::tuning::Tuning,
    material: Handle<TerrainMaterial>,
    water: Handle<WaterMaterial>,
    fresh: bool,
    /// Where the eye was last frame and how fast it is going, eased:
    /// which levels are worth streaming and how far ahead of itself the
    /// rings stand are both facts about that (`pace`).
    was: Option<DVec3>,
    pace: DVec3,
    pub stats: Stats,
    started: Instant,
    settled: Option<f32>,
    timings: Timings,
}

#[derive(Default)]
struct Timings {
    layout_ms: f32,
    max_layout_ms: f32,
    main_ms: f32,
    max_main_ms: f32,
    upload_ms: f32,
    publish_ms: f32,
    max_queue_ms: f32,
    layouts: usize,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct StreamAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    water: Res<'w, Assets<WaterMaterial>>,
}

/// What the streamer FOLLOWS: where the eye is, the frame it is drawn
/// in, the ground under it and how long this frame was.
///
/// One thing, because they are one question asked four ways and a fifth
/// would be the fifth argument on a system already at this project's own
/// limit. The clock joined them when the rings started adapting to how
/// fast the eye is going, which is a fact about the eye and not about
/// the world.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Watching<'w> {
    pub eye: Res<'w, Eye>,
    pub frame: Res<'w, Frame>,
    pub ground: Res<'w, Ground>,
    pub time: Res<'w, Time>,
}

impl Streamer {
    /// A streamer on `lat` with `levels` rings round `eye`, drawing with
    /// `material`, its workers started.
    pub fn new(
        lat: Lattice,
        eye: DVec3,
        levels: u8,
        material: Handle<TerrainMaterial>,
        water: Handle<WaterMaterial>,
        sampler: Option<Sampler>,
        tuning: crate::tuning::Tuning,
    ) -> Self {
        let workers = tuning.terrain_workers;
        let (jobs, done) = spawn_workers(workers, sampler);
        Streamer {
            centre: DVec3::ZERO,
            epoch: 0,
            lat,
            rings: Rings::around(&lat, eye, levels),
            wanted: HashMap::new(),
            loaded: HashMap::new(),
            staged: HashMap::new(),
            building: false,
            has_layout: false,
            planner: Planner::new(),
            planning: false,
            todo: VecDeque::new(),
            remaining: 0,
            retired: VecDeque::new(),
            pending: HashMap::new(),
            jobs,
            done: Mutex::new(done),
            workers,
            tuning,
            material,
            water,
            fresh: true,
            was: None,
            pace: DVec3::ZERO,
            stats: Stats::default(),
            started: Instant::now(),
            settled: None,
            timings: Timings::default(),
        }
    }

    /// Whether every wanted chunk is drawn.
    pub fn idle(&self) -> bool {
        !self.fresh && !self.building && self.pending.is_empty() && self.remaining == 0
    }

    /// Reuse the compute queue, but invalidate every result from the old body.
    pub(crate) fn change_body(
        &mut self,
        commands: &mut Commands,
        eye: DVec3,
        body: &crate::planets::Body,
    ) {
        for (_, cancelled) in self.pending.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.pending.clear();
        for (_, (entities, _, _)) in self.loaded.drain().chain(self.staged.drain()) {
            for entity in entities {
                commands.entity(entity).insert(Visibility::Hidden);
                self.retired.push_back(entity);
            }
        }
        self.epoch += 1;
        self.centre = body.centre;
        self.rings = Rings::around(&self.lat, eye - self.centre, self.rings.levels());
        self.wanted.clear();
        self.todo.clear();
        self.remaining = 0;
        self.planning = false;
        self.material = body.material.clone();
        self.water = body.water.clone();
        self.fresh = true;
        self.was = None;
        self.pace = DVec3::ZERO;
        self.building = false;
        self.has_layout = false;
        self.stats = Stats::default();
        self.started = Instant::now();
        self.settled = None;
        self.timings = Timings::default();
    }

    /// How fast the eye is going, eased, metres a second in the body's
    /// own frame.
    ///
    /// EASED over `PACE_EASE` and not the raw frame difference, because
    /// what it decides is which LEVELS are streamed, and a rule that
    /// read one frame's own step would drop and re-raise the finest ring
    /// every time a car touched the brake. A rebase never appears in it,
    /// since the eye here is planet local and an origin's move does not
    /// touch it; a body change resets it with everything else.
    fn pace(&mut self, eye: DVec3, dt: f64) {
        let raw = match (self.was.replace(eye), dt > 0.0) {
            (Some(last), true) => (eye - last) / dt,
            _ => DVec3::ZERO,
        };
        // A teleport is not a speed. Anything past what flight itself
        // allows is a jump (`--eye`, a body change, the first frame),
        // and the levels it would ask for are the coarsest there are
        // anyway.
        let raw = if raw.is_finite() { raw } else { DVec3::ZERO };
        let t = 1.0 - (-dt / PACE_EASE).exp();
        self.pace += (raw - self.pace) * t;
    }

    /// Follow the eye, and recompute the wanted set when a box moved.
    fn want(&mut self, eye: DVec3, world: &Arc<World>) {
        if self.building {
            return;
        }
        let height = (-world.planet.at(eye)).max(0.0);
        let adapted = self.rings.adapt(&self.lat, height, self.pace.length());
        // The boxes follow the eye's own GROUND at altitude, not the
        // eye: a box that is sixteen kilometres either way holds no
        // terrain at all once the eye is higher than that. And they
        // stand AHEAD of it by a second and a half of travel, so the
        // ground a car is driving into is streamed before the bonnet
        // reaches it rather than after.
        let focus = self.rings.focus(&self.lat, eye, height, self.pace);
        if !self.rings.follow(&self.lat, focus) && !adapted && !self.fresh {
            return;
        }
        let sent = self.planner.request(Request {
            epoch: self.epoch,
            lat: self.lat,
            rings: self.rings.clone(),
            eye,
            world: world.clone(),
        });
        self.fresh = !sent;
        self.building = sent;
        self.planning = sent;
    }

    fn accept_plan(&mut self) {
        while let Some(layout) = self.planner.poll() {
            if layout.epoch != self.epoch || !self.planning {
                continue;
            }
            self.begin_layout(layout);
        }
    }

    fn begin_layout(&mut self, layout: Layout) {
        self.timings.layout_ms = layout.ms;
        self.timings.max_layout_ms = self.timings.max_layout_ms.max(layout.ms);
        self.timings.layouts += 1;
        self.wanted = layout.wanted;
        self.todo.clear();
        let mut rebuilt = Vec::new();
        for id in layout.nearest {
            let sig = self.wanted[&id];
            if self.ready(&id, sig) {
                continue;
            }
            if self.loaded.contains_key(&id) {
                rebuilt.push((id, sig));
            } else {
                self.todo.push_back((id, sig));
            }
        }
        self.todo.extend(rebuilt);
        self.remaining = self.todo.len();
        self.stats.wanted = self.wanted.len();
        self.planning = false;
    }

    /// Send the nearest wanted chunks that are not loaded as wanted, new
    /// chunks before rebuilt ones, up to the queue's length.
    fn queue(&mut self, world: &Arc<World>) {
        let room =
            (self.workers + self.tuning.terrain_jobs_ahead).saturating_sub(self.pending.len());
        if room == 0 || self.planning || self.todo.is_empty() {
            return;
        }
        let rings = Arc::new(self.rings.clone());
        for _ in 0..room {
            let Some((id, sig)) = self.todo.pop_front() else {
                break;
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            self.pending.insert(id, (sig, cancelled.clone()));
            let mut job = Job::new(id, sig, self.lat, rings.clone(), world.clone(), cancelled);
            job.epoch = self.epoch;
            if self.jobs.send(job).is_err() {
                self.pending.remove(&id);
                self.todo.push_front((id, sig));
                warn!("the workers are gone");
                return;
            }
        }
    }

    /// The budget includes asset insertion and entity creation. CPU mesh
    /// conversion already happened on the workers, before this queue.
    fn drain(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        origin: &Origin,
        swell: f32,
    ) {
        let t0 = Instant::now();
        for _ in 0..self.tuning.terrain_upload_count {
            let done = self.done.lock().ok().and_then(|rx| rx.try_recv().ok());
            let Some(done) = done else {
                break;
            };
            if done.epoch != self.epoch {
                continue;
            }
            if self
                .pending
                .get(&done.id)
                .is_none_or(|(sig, _)| *sig != done.sig)
            {
                continue;
            }
            let pending = self.pending.remove(&done.id);
            self.stats.built += 1;
            self.stats.work_ms += done.ms;
            self.stats.last_ms = done.ms;
            let cancelled = pending.is_none_or(|(_, c)| c.load(Ordering::Relaxed));
            if !cancelled && self.wanted.get(&done.id) == Some(&done.sig) {
                self.install(done, commands, meshes, origin, swell);
            }
            if t0.elapsed().as_secs_f32() * 1000.0 >= self.tuning.terrain_upload_ms {
                break;
            }
        }
        self.timings.upload_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    fn install(
        &mut self,
        done: Done,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        origin: &Origin,
        swell: f32,
    ) {
        let corner = WorldPos(self.centre + done.id.corner(&self.lat));
        let at = Transform::from_translation(origin.local(corner));
        let visibility = if self.has_layout {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        let mut entities = Vec::new();
        if let Some(mesh) = done.mesh {
            let bounds = mesh.compute_aabb();
            let mut entity = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(self.material.clone()),
                at,
                Anchored { at: corner },
                visibility,
                TerrainLod(done.id.level),
            ));
            if let Some(bounds) = bounds {
                entity.insert((bounds, NoAutoAabb));
            }
            entities.push(entity.id());
        }
        if let Some(sheet) = done.sheet {
            let bounds = sheet.compute_aabb().map(|mut bounds| {
                bounds.half_extents += bevy::math::Vec3A::splat(swell);
                bounds
            });
            let mut entity = commands.spawn((
                Mesh3d(meshes.add(sheet)),
                MeshMaterial3d(self.water.clone()),
                at,
                Anchored { at: corner },
                visibility,
                Sheet,
            ));
            if let Some(bounds) = bounds {
                entity.insert((bounds, NoAutoAabb));
            }
            entities.push(entity.id());
        }
        let destination = if self.has_layout {
            &mut self.staged
        } else {
            self.stats.triangles += done.triangles;
            &mut self.loaded
        };
        destination.insert(done.id, (entities, done.sig, done.triangles));
        self.remaining = self.remaining.saturating_sub(1);
    }

    fn ready(&self, id: &ChunkId, sig: u64) -> bool {
        self.staged
            .get(id)
            .or_else(|| self.loaded.get(id))
            .is_some_and(|l| l.1 == sig)
    }

    /// Both sides change visibility in one deferred-command flush. A seam
    /// belongs to a neighbor as well as the chunk it covers, so spatial
    /// coverage alone is insufficient to retire any part of the old layout.
    fn publish(&mut self, commands: &mut Commands) {
        let t0 = Instant::now();
        if self.building && !self.planning && self.remaining == 0 {
            let gone: Vec<_> = self
                .loaded
                .keys()
                .filter(|id| !self.wanted.contains_key(id) || self.staged.contains_key(id))
                .copied()
                .collect();
            for id in gone {
                if let Some((entities, _, triangles)) = self.loaded.remove(&id) {
                    self.stats.triangles = self.stats.triangles.saturating_sub(triangles);
                    for e in entities {
                        commands.entity(e).insert(Visibility::Hidden);
                        self.retired.push_back(e);
                    }
                }
            }
            for (id, loaded) in self.staged.drain() {
                for &entity in &loaded.0 {
                    commands.entity(entity).insert(Visibility::Inherited);
                }
                self.stats.triangles += loaded.2;
                self.loaded.insert(id, loaded);
            }
            self.building = false;
            self.has_layout = true;
        }
        self.stats.loaded = self.loaded.len();
        self.stats.pending = self.pending.len();
        self.timings.publish_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    fn retire(&mut self, commands: &mut Commands) {
        // An upload can create both ground and water. Retirement must keep
        // pace with that maximum production rate during continuous motion.
        for _ in 0..self.tuning.terrain_upload_count.saturating_mul(2) {
            let Some(entity) = self.retired.pop_front() else {
                break;
            };
            commands.entity(entity).despawn();
        }
    }

    /// One line for the status bar.
    pub fn status(&self) -> String {
        let s = &self.stats;
        format!(
            "{} chunks, {} triangles, {} pending, {:.0} ms a chunk",
            s.loaded,
            s.triangles,
            s.pending,
            if s.built > 0 {
                s.work_ms / s.built as f32
            } else {
                0.0
            }
        )
    }

    pub fn measurement(&self) -> serde_json::Value {
        serde_json::json!({"settled_seconds": self.settled, "loaded": self.stats.loaded,
            "staged": self.staged.len(), "transitioning": self.building,
            "planning": self.planning, "remaining": self.remaining, "retired": self.retired.len(),
            "wanted": self.stats.wanted, "pending": self.pending.len(), "triangles": self.stats.triangles,
            "built": self.stats.built, "workers": self.workers, "work_ms": self.stats.work_ms,
            "mean_chunk_ms": self.stats.work_ms / self.stats.built.max(1) as f32,
            "layouts": self.timings.layouts, "layout_worker_ms": self.timings.layout_ms,
            "max_layout_worker_ms": self.timings.max_layout_ms,
            "max_queue_ms": self.timings.max_queue_ms,
            "stream_main_ms": self.timings.main_ms, "max_stream_main_ms": self.timings.max_main_ms,
            "upload_ms": self.timings.upload_ms, "publish_ms": self.timings.publish_ms})
    }
}

/// Move the origin after the eye, and every chunk and the planet's centre
/// with it.
pub fn rebase_origin(
    eye: Res<Eye>,
    mut frame: ResMut<Frame>,
    mut drawn: Query<(&Anchored, &mut Transform)>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    streamer: Option<Res<Streamer>>,
) {
    // The first frame is a follow like any other: the eye is a planet's
    // radius from an origin at nought, so the origin snaps to its cell.
    if !frame.0.follow(eye.0) {
        return;
    }
    for (a, mut tf) in &mut drawn {
        tf.translation = frame.0.local(a.at);
    }
    if let Some(streamer) = streamer {
        let centre = frame.0.local(WorldPos(streamer.centre));
        // EVERY terrain material and not the streamer's own handle: the
        // tarmac wears a second one, identical but for its depth bias,
        // and a body's centre written into one of the two would be a
        // road mapped and fogged in a frame the ground left behind at
        // the last rebase. `sky.rs` already writes the sun and the air
        // this way and for the same reason.
        let ids: Vec<_> = materials.ids().collect();
        for id in ids {
            if let Some(m) = materials.get_mut(id) {
                m.extension.centre = centre.extend(0.0);
            }
        }
        recentre(&mut waters, &streamer.water, centre);
    }
    info!("origin at {:.0}", frame.0.at);
}

/// One frame of streaming: follow the eye, queue, drain, publish, and log
/// when the first load has settled.
pub fn stream(
    mut commands: Commands,
    mut assets: StreamAssets,
    mut streamer: ResMut<Streamer>,
    watching: Watching,
    debug: Res<LodDebug>,
) {
    let Watching {
        eye,
        frame,
        ground,
        time,
    } = &watching;
    let t0 = Instant::now();
    streamer.retire(&mut commands);
    streamer.accept_plan();
    // The pace EVERY frame, and the wanted set only when a box moved:
    // `want` returns early while a layout is building, and a speed that
    // was only measured when it did not would be the speed the eye had
    // the last time the streamer looked.
    let here = eye.0 .0 - ground.1;
    streamer.pace(here, time.delta_secs_f64());
    if !debug.frozen || streamer.fresh {
        streamer.want(here, &ground.0);
    }
    let queue_start = Instant::now();
    streamer.queue(&ground.0);
    streamer.timings.max_queue_ms = streamer
        .timings
        .max_queue_ms
        .max(queue_start.elapsed().as_secs_f32() * 1000.0);
    let swell = assets.water.get(&streamer.water).map_or(0.0, |material| {
        crate::water::swell_bound(&material.extension)
    });
    streamer.drain(&mut commands, &mut assets.meshes, &frame.0, swell);
    streamer.publish(&mut commands);
    streamer.timings.main_ms = t0.elapsed().as_secs_f32() * 1000.0;
    streamer.timings.max_main_ms = streamer.timings.max_main_ms.max(streamer.timings.main_ms);
    if streamer.settled.is_none() && streamer.idle() && streamer.stats.built > 0 {
        let took = streamer.started.elapsed().as_secs_f32();
        streamer.settled = Some(took);
        let s = &streamer.stats;
        info!(
            "settled in {:.1} s: {} chunks of {} wanted, {} triangles, {} built on {} workers in {:.0} ms of work, {:.1} ms a chunk",
            took, s.loaded, s.wanted, s.triangles, s.built, streamer.workers, s.work_ms, s.work_ms / s.built as f32
        );
    }
}

#[cfg(test)]
mod tests;
