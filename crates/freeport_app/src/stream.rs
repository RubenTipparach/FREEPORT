//! The chunk streamer: the chunks the rings want, contoured on workers, and
//! drawn through the floating origin.
//!
//! Every frame the rings follow the eye (`Rings::follow`) and, when a box
//! moves, the wanted set is recomputed: every chunk of every level's box
//! that the field cannot rule wholly rock or air, each with the signature
//! of its neighbours' levels, which is everything its mesh depends on
//! besides the field. A wanted chunk that is not loaded with that
//! signature is a job, nearest first and new before rebuilt, contoured on
//! a worker thread from the same field and the same rings, and drawn when
//! it comes back if it is still wanted as it was. A replacement layout is
//! uploaded hidden, then published together once all its seams are ready.
//! The previous layout remains visible until that swap. The target rings
//! stay fixed during a build, so moving cannot keep cancelling a transition.
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
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::field::Density;
use freeport_core::lattice::{ChunkId, Lattice, Rings};
use freeport_core::pos::{Origin, WorldPos};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
    pub stats: Stats,
    started: Instant,
    settled: Option<f32>,
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
            pending: HashMap::new(),
            jobs,
            done: Mutex::new(done),
            workers,
            tuning,
            material,
            water,
            fresh: true,
            stats: Stats::default(),
            started: Instant::now(),
            settled: None,
        }
    }

    /// Whether every wanted chunk is drawn.
    pub fn idle(&self) -> bool {
        !self.building
            && self.pending.is_empty()
            && self
                .wanted
                .iter()
                .all(|(id, sig)| self.loaded.get(id).is_some_and(|l| l.1 == *sig))
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
                commands.entity(entity).despawn();
            }
        }
        self.epoch += 1;
        self.centre = body.centre;
        self.rings = Rings::around(&self.lat, eye - self.centre, self.rings.levels());
        self.wanted.clear();
        self.material = body.material.clone();
        self.water = body.water.clone();
        self.fresh = true;
        self.building = false;
        self.has_layout = false;
        self.stats = Stats::default();
        self.started = Instant::now();
        self.settled = None;
    }

    /// Follow the eye, and recompute the wanted set when a box moved.
    fn want(&mut self, eye: DVec3, world: &World) {
        if self.building {
            return;
        }
        let adapted = self
            .rings
            .adapt(&self.lat, (-world.planet.at(eye)).max(0.0));
        if !self.rings.follow(&self.lat, eye) && !adapted && !self.fresh {
            return;
        }
        self.fresh = false;
        self.building = true;
        self.wanted.clear();
        for id in self.rings.chunks() {
            let (lo, hi) = id.bounds(&self.lat, 0);
            // The GROUND alone, which is all a chunk is contoured on:
            // what is built on a town is a model beside the field rather
            // than a brush in it, so a chunk under a city is ruled the
            // same way a chunk in the wilderness is.
            let ground = world.ground();
            if ground.solid(lo, hi).is_some() && world.water(&ground).solid(lo, hi).is_some() {
                continue;
            }
            self.wanted.insert(id, self.rings.signature(id));
        }
        for (id, (sig, cancelled)) in &self.pending {
            if self.wanted.get(id) != Some(sig) {
                cancelled.store(true, Ordering::Relaxed);
            }
        }
        self.stats.wanted = self.wanted.len();
    }

    /// Send the nearest wanted chunks that are not loaded as wanted, new
    /// chunks before rebuilt ones, up to the queue's length.
    fn queue(&mut self, eye: DVec3, world: &Arc<World>) {
        let room =
            (self.workers + self.tuning.terrain_jobs_ahead).saturating_sub(self.pending.len());
        if room == 0 {
            return;
        }
        let mut todo: Vec<(bool, f64, ChunkId, u64)> = self
            .wanted
            .iter()
            .filter(|(id, sig)| !self.ready(id, **sig) && !self.pending.contains_key(id))
            .map(|(id, sig)| {
                let c = id.corner(&self.lat) + DVec3::splat(id.size(&self.lat) * 0.5);
                (self.loaded.contains_key(id), (c - eye).length(), *id, *sig)
            })
            .collect();
        todo.sort_by(|a, b| {
            (a.0, a.1)
                .partial_cmp(&(b.0, b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let rings = Arc::new(self.rings.clone());
        for (_, _, id, sig) in todo.into_iter().take(room) {
            let cancelled = Arc::new(AtomicBool::new(false));
            self.pending.insert(id, (sig, cancelled.clone()));
            let mut job = Job::new(id, sig, self.lat, rings.clone(), world.clone(), cancelled);
            job.epoch = self.epoch;
            if self.jobs.send(job).is_err() {
                warn!("the workers are gone");
                return;
            }
        }
    }

    /// The budget includes asset insertion and entity creation. CPU mesh
    /// conversion already happened on the workers, before this queue.
    fn drain(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, origin: &Origin) {
        let t0 = Instant::now();
        for _ in 0..self.tuning.terrain_upload_count {
            let done = self.done.lock().ok().and_then(|rx| rx.try_recv().ok());
            let Some(done) = done else {
                break;
            };
            if done.epoch != self.epoch {
                continue;
            }
            let pending = self.pending.remove(&done.id);
            self.stats.built += 1;
            self.stats.work_ms += done.ms;
            self.stats.last_ms = done.ms;
            let cancelled = pending.is_none_or(|(_, c)| c.load(Ordering::Relaxed));
            if !cancelled && self.wanted.get(&done.id) == Some(&done.sig) {
                self.install(done, commands, meshes, origin);
            }
            if t0.elapsed().as_secs_f32() * 1000.0 >= self.tuning.terrain_upload_ms {
                break;
            }
        }
    }

    fn install(
        &mut self,
        done: Done,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        origin: &Origin,
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
            entities.push(
                commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(self.material.clone()),
                        at,
                        Anchored { at: corner },
                        visibility,
                        TerrainLod(done.id.level),
                    ))
                    .id(),
            );
        }
        if let Some(sheet) = done.sheet {
            entities.push(
                commands
                    .spawn((
                        Mesh3d(meshes.add(sheet)),
                        MeshMaterial3d(self.water.clone()),
                        at,
                        Anchored { at: corner },
                        visibility,
                        Sheet,
                    ))
                    .id(),
            );
        }
        let destination = if self.has_layout {
            &mut self.staged
        } else {
            &mut self.loaded
        };
        destination.insert(done.id, (entities, done.sig, done.triangles));
    }

    fn ready(&self, id: &ChunkId, sig: u64) -> bool {
        self.staged
            .get(id)
            .or_else(|| self.loaded.get(id))
            .is_some_and(|l| l.1 == sig)
    }

    /// Visibility and removal happen in one deferred-command flush. A seam
    /// belongs to a neighbor as well as the chunk it covers, so spatial
    /// coverage alone is insufficient to retire any part of the old layout.
    fn publish(&mut self, commands: &mut Commands) {
        if self.building && self.wanted.iter().all(|(id, sig)| self.ready(id, *sig)) {
            let gone: Vec<_> = self
                .loaded
                .keys()
                .filter(|id| !self.wanted.contains_key(id) || self.staged.contains_key(id))
                .copied()
                .collect();
            for id in gone {
                if let Some((entities, _, _)) = self.loaded.remove(&id) {
                    for e in entities {
                        commands.entity(e).despawn();
                    }
                }
            }
            for (id, loaded) in self.staged.drain() {
                for &entity in &loaded.0 {
                    commands.entity(entity).insert(Visibility::Inherited);
                }
                self.loaded.insert(id, loaded);
            }
            self.building = false;
            self.has_layout = true;
        }
        self.stats.loaded = self.loaded.len();
        self.stats.pending = self.pending.len();
        self.stats.triangles = self.loaded.values().map(|l| l.2).sum();
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
            "wanted": self.stats.wanted, "pending": self.pending.len(), "triangles": self.stats.triangles,
            "built": self.stats.built, "workers": self.workers, "work_ms": self.stats.work_ms,
            "mean_chunk_ms": self.stats.work_ms / self.stats.built.max(1) as f32})
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
        if let Some(m) = materials.get_mut(&streamer.material) {
            m.extension.centre = centre.extend(0.0);
        }
        recentre(&mut waters, &streamer.water, centre);
    }
    info!("origin at {:.0}", frame.0.at);
}

/// One frame of streaming: follow the eye, queue, drain, publish, and log
/// when the first load has settled.
pub fn stream(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut streamer: ResMut<Streamer>,
    ground: Res<Ground>,
    eye: Res<Eye>,
    frame: Res<Frame>,
    debug: Res<LodDebug>,
) {
    if !debug.frozen || streamer.fresh {
        streamer.want(eye.0 .0 - ground.1, &ground.0);
    }
    streamer.queue(eye.0 .0 - ground.1, &ground.0);
    streamer.drain(&mut commands, &mut meshes, &frame.0);
    streamer.publish(&mut commands);
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
