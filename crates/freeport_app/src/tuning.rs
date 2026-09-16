//! Streaming and building LOD budgets. Zero selects the documented default.
use bevy::prelude::*;
use serde::Deserialize;

#[derive(Resource, Clone, Deserialize)]
#[serde(default)]
pub struct Tuning {
    pub terrain_cell_size: f64,
    pub terrain_upload_ms: f32,
    pub terrain_upload_count: usize,
    pub terrain_jobs_ahead: usize,
    pub terrain_workers: usize,
    pub terrain_compute_batch: usize,
    pub building_lod_near: f64,
    pub building_lod_far: f64,
    pub lod_hysteresis: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            terrain_cell_size: 0.5,
            terrain_upload_ms: 3.0,
            terrain_upload_count: 16,
            terrain_jobs_ahead: 32,
            terrain_workers: std::thread::available_parallelism()
                .map(|n| (n.get() / 2).clamp(1, 8))
                .unwrap_or(2),
            terrain_compute_batch: 2,
            building_lod_near: 250.0,
            building_lod_far: 1200.0,
            lod_hysteresis: 0.15,
        }
    }
}

impl Tuning {
    pub fn load() -> Self {
        let parsed = crate::terrain::assets_dir()
            .and_then(|p| std::fs::read(p.join("config/render.json")).ok())
            .and_then(|bytes| match serde_json::from_slice::<Self>(&bytes) {
                Ok(v) => Some(v),
                Err(e) => {
                    warn!("invalid render settings: {e}; using defaults");
                    None
                }
            });
        parsed.unwrap_or_default().validated()
    }

    fn validated(mut self) -> Self {
        let d = Self::default();
        if !self.terrain_cell_size.is_finite() || self.terrain_cell_size <= 0.0 {
            self.terrain_cell_size = d.terrain_cell_size;
        }
        self.terrain_cell_size = self.terrain_cell_size.clamp(0.125, 4.0);
        if !self.terrain_upload_ms.is_finite() || self.terrain_upload_ms <= 0.0 {
            self.terrain_upload_ms = d.terrain_upload_ms;
        }
        if self.terrain_upload_count == 0 {
            self.terrain_upload_count = d.terrain_upload_count;
        }
        if self.terrain_jobs_ahead == 0 {
            self.terrain_jobs_ahead = d.terrain_jobs_ahead;
        }
        if self.terrain_workers == 0 {
            self.terrain_workers = d.terrain_workers;
        }
        if self.terrain_compute_batch == 0 {
            self.terrain_compute_batch = d.terrain_compute_batch;
        }
        self.terrain_compute_batch = self.terrain_compute_batch.min(crate::compute::BATCH);
        self.terrain_workers = self.terrain_workers.min(64);
        self.terrain_upload_count = self.terrain_upload_count.min(256);
        self.terrain_jobs_ahead = self.terrain_jobs_ahead.min(256);
        if !self.building_lod_near.is_finite() || self.building_lod_near <= 0.0 {
            self.building_lod_near = d.building_lod_near;
        }
        if !self.building_lod_far.is_finite() || self.building_lod_far <= self.building_lod_near {
            self.building_lod_far = d.building_lod_far.max(self.building_lod_near * 2.0);
        }
        if !self.lod_hysteresis.is_finite() || self.lod_hysteresis <= 0.0 {
            self.lod_hysteresis = d.lod_hysteresis;
        }
        self.lod_hysteresis = self.lod_hysteresis.min(0.4);
        self
    }
}
