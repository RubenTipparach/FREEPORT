//! Bounded sampling and meshing workers, with cancellation before costly work.

use crate::compute::Sampler;
use crate::terrain::{chunk_mapping, to_mesh};
use crate::water::to_sheet;
use crate::World;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use freeport_core::dc::{contour, contour_sampled, DcMesh};
use freeport_core::field::Density;
use freeport_core::lattice::{ChunkId, Lattice, Rings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Job {
    pub epoch: u64,
    pub id: ChunkId,
    pub sig: u64,
    pub lat: Lattice,
    pub rings: Arc<Rings>,
    pub world: Arc<World>,
    pub cancelled: Arc<AtomicBool>,
    samples: Option<Vec<f32>>,
    sample_ms: f32,
}

pub struct Done {
    pub epoch: u64,
    pub id: ChunkId,
    pub sig: u64,
    pub mesh: Option<Mesh>,
    pub sheet: Option<Mesh>,
    pub triangles: usize,
    pub ms: f32,
}

impl Job {
    /// A fully levelled site has no procedural noise to offload. Those
    /// chunks go straight to the CPU workers and can overlap a GPU batch.
    fn needs_gpu(&self) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return false;
        }
        let (lo, hi) = self.id.bounds(&self.lat, 0);
        if self.world.planet.solid(lo, hi).is_some() {
            return false;
        }
        let (lo, hi) = self.id.bounds(&self.lat, freeport_core::lattice::MARGIN);
        (0..8).any(|corner| {
            let p = bevy::math::DVec3::new(
                if corner & 1 == 0 { lo.x } else { hi.x },
                if corner & 2 == 0 { lo.y } else { hi.y },
                if corner & 4 == 0 { lo.z } else { hi.z },
            );
            self.world
                .planet
                .surface_blend(p.normalize_or(bevy::math::DVec3::Y))
                .1
                > 0.0
        })
    }

    pub fn new(
        id: ChunkId,
        sig: u64,
        lat: Lattice,
        rings: Arc<Rings>,
        world: Arc<World>,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            epoch: 0,
            id,
            sig,
            lat,
            rings,
            world,
            cancelled,
            samples: None,
            sample_ms: 0.0,
        }
    }

    fn run(self) -> Done {
        let mut done = Done {
            epoch: self.epoch,
            id: self.id,
            sig: self.sig,
            mesh: None,
            sheet: None,
            triangles: 0,
            ms: 0.0,
        };
        if self.cancelled.load(Ordering::Relaxed) {
            return done;
        }
        let t0 = Instant::now();
        let (lo, hi) = self.id.bounds(&self.lat, 0);
        let field = self.world.ground();
        let mesh = if field.solid(lo, hi).is_some() {
            DcMesh::default()
        } else if let Some(samples) = self.samples {
            contour_sampled(&field, &self.lat, self.id, &*self.rings, samples)
        } else {
            contour(&field, &self.lat, self.id, &*self.rings)
        };
        if self.cancelled.load(Ordering::Relaxed) {
            return done;
        }
        if mesh.triangles() > 0 {
            done.triangles += mesh.triangles();
            done.mesh = Some(to_mesh(
                &mesh,
                chunk_mapping(
                    self.id.corner(&self.lat),
                    self.world.sea.radius,
                    Some(self.world.planet.shape()),
                ),
            ));
        }
        let water = self.world.water(&field);
        if water.solid(lo, hi).is_none() {
            done.sheet = to_sheet(
                &contour(&water, &self.lat, self.id, &*self.rings),
                self.id.corner(&self.lat),
            );
            done.triangles += done
                .sheet
                .as_ref()
                .and_then(|m| m.indices())
                .map_or(0, |i| i.len() / 3);
        }
        for mesh in done.mesh.iter_mut().chain(done.sheet.iter_mut()) {
            mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
        done.ms = self.sample_ms + t0.elapsed().as_secs_f32() * 1000.0;
        done
    }
}

pub fn spawn_workers(n: usize, sampler: Option<Sampler>) -> (Sender<Job>, Receiver<Done>) {
    let (jobs, take) = channel::<Job>();
    let (give, done) = channel::<Done>();
    let take = if let Some(sampler) = sampler {
        let (sampled, ready) = channel();
        std::thread::spawn(move || sample_batches(take, sampled, sampler));
        ready
    } else {
        take
    };
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
            if give.send(job.run()).is_err() {
                return;
            }
        });
    }
    (jobs, done)
}

fn sample_batches(take: Receiver<Job>, sampled: Sender<Job>, sampler: Sampler) {
    let mut enabled = true;
    while let Ok(first) = take.recv() {
        let mut incoming = vec![first];
        incoming.extend(take.try_iter().take(sampler.batch_limit - 1));
        let mut batches: Vec<Vec<Job>> = Vec::new();
        for job in incoming {
            if enabled && job.needs_gpu() {
                if let Some(batch) = batches
                    .iter_mut()
                    .find(|b| Arc::ptr_eq(&b[0].world, &job.world))
                {
                    batch.push(job);
                } else {
                    batches.push(vec![job]);
                }
            } else if sampled.send(job).is_err() {
                return;
            }
        }
        for mut batch in batches {
            let t0 = Instant::now();
            let values = sample_if_enabled(&mut enabled, || {
                let chunks: Vec<_> = batch.iter().map(|job| (job.lat, job.id)).collect();
                sampler.sample(&batch[0].world.planet, &chunks)
            });
            if let Some(values) = values {
                let ms = t0.elapsed().as_secs_f32() * 1000.0 / batch.len() as f32;
                for (job, samples) in batch.iter_mut().zip(values) {
                    job.samples = Some(samples);
                    job.sample_ms = ms;
                }
            }
            for job in batch {
                if sampled.send(job).is_err() {
                    return;
                }
            }
        }
    }
}

/// A failed readback destroys its buffer. Disable the sampler immediately,
/// including remaining world-specific batches from this same queue drain.
fn sample_if_enabled(
    enabled: &mut bool,
    sample: impl FnOnce() -> Result<Vec<Vec<f32>>, String>,
) -> Option<Vec<Vec<f32>>> {
    if !*enabled {
        return None;
    }
    match sample() {
        Ok(values) => Some(values),
        Err(error) => {
            warn!("GPU density sampling failed; using CPU: {error}");
            *enabled = false;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sample_if_enabled;

    #[test]
    fn failed_gpu_batch_never_reuses_the_destroyed_sampler() {
        let mut enabled = true;
        assert_eq!(
            sample_if_enabled(&mut enabled, || Ok(vec![vec![1.0]])),
            Some(vec![vec![1.0]])
        );
        assert!(enabled);
        assert!(sample_if_enabled(&mut enabled, || Err("readback failed".into())).is_none());
        assert!(!enabled);
        for _ in 0..3 {
            assert!(sample_if_enabled(&mut enabled, || {
                panic!("the failed sampler cannot serve later batches")
            })
            .is_none());
        }
    }
}
