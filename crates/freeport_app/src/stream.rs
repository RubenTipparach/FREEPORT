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
//! it comes back if it is still wanted as it was. A loaded chunk the rings
//! no longer want stays drawn until whatever now covers its ground has
//! arrived, so the ground never has a hole where a level changes, and for
//! three seconds at most, so nothing stays for ever.
//!
//! A chunk's mesh is `f32` metres from its own `f64` corner, and its
//! entity is placed from that corner through the origin (`pos::Origin`),
//! which follows the eye and moves every chunk with it when it does. The
//! subtraction is the precise step and it happens once, in `f64`.

use crate::terrain::{to_mesh, TerrainMaterial};
use crate::water::{recentre, to_sheet, Sheet, WaterMaterial};
use crate::{Eye, Ground, World};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::dc::{contour, DcMesh};
use freeport_core::field::Density;
use freeport_core::lattice::{ChunkId, Lattice, Rings};
use freeport_core::pos::{Origin, WorldPos};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
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

/// How long a chunk nobody wants is drawn before it goes whatever covers
/// it, seconds.
const LINGER: f32 = 3.0;
/// Milliseconds a frame spends taking meshes off the workers, and the
/// fewest it takes whatever they cost: a count alone throttled the first
/// load to the frame rate, two thousand chunks at a dozen a frame.
const DRAIN_MS: f32 = 6.0;
const DRAIN_LEAST: usize = 32;
/// Jobs in flight beyond the workers, so a worker never waits for the
/// main thread and a stale job is never far down the queue.
const AHEAD: usize = 64;

struct Job {
    id: ChunkId,
    sig: u64,
    lat: Lattice,
    rings: Arc<Rings>,
    world: Arc<World>,
}

struct Done {
    id: ChunkId,
    sig: u64,
    mesh: DcMesh,
    /// The sea's surface through the chunk, contoured on the same cells.
    sheet: DcMesh,
    ms: f32,
}

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

#[derive(Resource)]
pub struct Streamer {
    pub lat: Lattice,
    pub rings: Rings,
    wanted: HashMap<ChunkId, u64>,
    /// What is drawn for a chunk: its entities (ground, and the sea's
    /// sheet where there is one), the signature it was built for, and its
    /// triangles.
    loaded: HashMap<ChunkId, (Vec<Entity>, u64, usize)>,
    pending: HashMap<ChunkId, u64>,
    stale: HashMap<ChunkId, Instant>,
    jobs: Sender<Job>,
    /// Behind a mutex only because a resource must be `Sync`; the main
    /// thread is the one reader.
    done: Mutex<Receiver<Done>>,
    workers: usize,
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
    ) -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).max(1))
            .unwrap_or(2);
        let (jobs, done) = spawn_workers(workers);
        Streamer {
            lat,
            rings: Rings::around(&lat, eye, levels),
            wanted: HashMap::new(),
            loaded: HashMap::new(),
            pending: HashMap::new(),
            stale: HashMap::new(),
            jobs,
            done: Mutex::new(done),
            workers,
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
        self.pending.is_empty()
            && self
                .wanted
                .iter()
                .all(|(id, sig)| self.loaded.get(id).is_some_and(|l| l.1 == *sig))
    }

    /// Follow the eye, and recompute the wanted set when a box moved.
    fn want(&mut self, eye: DVec3, world: &World) {
        if !self.rings.follow(&self.lat, eye) && !self.fresh {
            return;
        }
        self.fresh = false;
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
        self.stats.wanted = self.wanted.len();
    }

    /// Send the nearest wanted chunks that are not loaded as wanted, new
    /// chunks before rebuilt ones, up to the queue's length.
    fn queue(&mut self, eye: DVec3, world: &Arc<World>) {
        let room = (self.workers + AHEAD).saturating_sub(self.pending.len());
        if room == 0 {
            return;
        }
        let mut todo: Vec<(bool, f64, ChunkId, u64)> = self
            .wanted
            .iter()
            .filter(|(id, sig)| {
                self.loaded.get(id).is_none_or(|l| l.1 != **sig)
                    && self.pending.get(id) != Some(sig)
            })
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
            self.pending.insert(id, sig);
            let job = Job {
                id,
                sig,
                lat: self.lat,
                rings: rings.clone(),
                world: world.clone(),
            };
            if self.jobs.send(job).is_err() {
                warn!("the workers are gone");
                return;
            }
        }
    }

    /// Take finished meshes off the workers and draw the ones still wanted.
    fn drain(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, origin: &Origin) {
        let Ok(rx) = self.done.lock() else {
            return;
        };
        let t0 = Instant::now();
        let mut finished = Vec::new();
        while finished.len() < DRAIN_LEAST || t0.elapsed().as_secs_f32() * 1000.0 < DRAIN_MS {
            let Ok(done) = rx.try_recv() else {
                break;
            };
            finished.push(done);
        }
        drop(rx);
        for done in finished {
            // A result nobody is waiting for: the job was resent
            // on the new world and is still pending.
            if self.pending.get(&done.id) == Some(&done.sig) {
                self.pending.remove(&done.id);
            }
            self.stats.built += 1;
            self.stats.work_ms += done.ms;
            self.stats.last_ms = done.ms;
            if self.wanted.get(&done.id) != Some(&done.sig) {
                continue;
            }
            if let Some((old, _, _)) = self.loaded.remove(&done.id) {
                for e in old {
                    commands.entity(e).despawn();
                }
            }
            let corner = WorldPos(done.id.corner(&self.lat));
            let at = Transform::from_translation(origin.local(corner));
            let chunk = || Anchored { at: corner };
            let mut entities = Vec::new();
            let mut triangles = 0;
            if done.mesh.triangles() > 0 {
                triangles += done.mesh.triangles();
                entities.push(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(to_mesh(&done.mesh))),
                            MeshMaterial3d(self.material.clone()),
                            at,
                            chunk(),
                        ))
                        .id(),
                );
            }
            if let Some(sheet) = to_sheet(&done.sheet) {
                triangles += sheet.indices().map_or(0, |i| i.len() / 3);
                entities.push(
                    commands
                        .spawn((
                            Mesh3d(meshes.add(sheet)),
                            MeshMaterial3d(self.water.clone()),
                            at,
                            chunk(),
                            Sheet,
                        ))
                        .id(),
                );
            }
            self.loaded.insert(done.id, (entities, done.sig, triangles));
        }
    }

    /// Whether the ground a chunk covered is drawn by what the rings want
    /// there now: the chunk above it, or the chunks below it, two levels
    /// down at most.
    fn covered(&self, id: ChunkId, depth: u8) -> bool {
        if let Some(sig) = self.wanted.get(&id) {
            return self.loaded.get(&id).is_some_and(|l| l.1 == *sig);
        }
        if depth == 0 {
            let above = ChunkId {
                level: id.level + 1,
                at: id.at.map(|v| v.div_euclid(2)),
            };
            if self.wanted.contains_key(&above) {
                return self.loaded.contains_key(&above);
            }
        }
        if id.level == 0 || depth >= 2 {
            return true;
        }
        (0..8).all(|c| {
            let below = ChunkId {
                level: id.level - 1,
                at: [
                    2 * id.at[0] + (c & 1),
                    2 * id.at[1] + ((c >> 1) & 1),
                    2 * id.at[2] + (c >> 2),
                ],
            };
            self.covered(below, depth + 1)
        })
    }

    /// Despawn the chunks nobody wants once what covers them is drawn, or
    /// once they have lingered.
    fn prune(&mut self, commands: &mut Commands) {
        let now = Instant::now();
        let gone: Vec<ChunkId> = self
            .loaded
            .keys()
            .filter(|id| !self.wanted.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            let since = *self.stale.entry(id).or_insert(now);
            if self.covered(id, 0) || now.duration_since(since).as_secs_f32() > LINGER {
                if let Some((entities, _, _)) = self.loaded.remove(&id) {
                    for e in entities {
                        commands.entity(e).despawn();
                    }
                }
                self.stale.remove(&id);
            }
        }
        self.stale.retain(|id, _| self.loaded.contains_key(id));
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
}

/// Start `n` workers pulling jobs off one queue.
fn spawn_workers(n: usize) -> (Sender<Job>, Receiver<Done>) {
    let (jobs, take) = channel::<Job>();
    let (give, done) = channel::<Done>();
    let take = Arc::new(Mutex::new(take));
    for _ in 0..n {
        let take = take.clone();
        let give = give.clone();
        std::thread::spawn(move || loop {
            let job = match take.lock() {
                Ok(rx) => rx.recv(),
                Err(_) => return,
            };
            let Ok(job) = job else {
                return;
            };
            let t0 = Instant::now();
            let (lo, hi) = job.id.bounds(&job.lat, 0);
            let field = job.world.ground();
            let mesh = if field.solid(lo, hi).is_none() {
                contour(&field, &job.lat, job.id, &*job.rings)
            } else {
                DcMesh::default()
            };
            let water = job.world.water(&field);
            let sheet = if water.solid(lo, hi).is_none() {
                contour(&water, &job.lat, job.id, &*job.rings)
            } else {
                DcMesh::default()
            };
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            if give
                .send(Done {
                    id: job.id,
                    sig: job.sig,
                    mesh,
                    sheet,
                    ms,
                })
                .is_err()
            {
                return;
            }
        });
    }
    (jobs, done)
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
        let centre = frame.0.local(WorldPos(DVec3::ZERO));
        if let Some(m) = materials.get_mut(&streamer.material) {
            m.extension.centre = centre.extend(0.0);
        }
        recentre(&mut waters, &streamer.water, centre);
    }
    info!("origin at {:.0}", frame.0.at);
}

/// One frame of streaming: follow the eye, queue, drain, prune, and log
/// when the first load has settled.
pub fn stream(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut streamer: ResMut<Streamer>,
    ground: Res<Ground>,
    eye: Res<Eye>,
    frame: Res<Frame>,
) {
    streamer.want(eye.0 .0, &ground.0);
    streamer.queue(eye.0 .0, &ground.0);
    streamer.drain(&mut commands, &mut meshes, &frame.0);
    streamer.prune(&mut commands);
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
