//! Opt-in render-pass and GPU mesh-allocation measurements for flight profiling.

use bevy::diagnostic::DiagnosticsStore;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::diagnostic::{MeshAllocatorDiagnosticPlugin, RenderDiagnosticsPlugin};
use std::collections::BTreeMap;

/// Add after DefaultPlugins only for an explicitly requested diagnostic run.
pub(crate) struct RenderProbePlugin;

impl Plugin for RenderProbePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RenderDiagnosticsPlugin, MeshAllocatorDiagnosticPlugin))
            .init_resource::<RenderProbe>()
            .add_systems(Last, collect);
    }
}

/// Call `start` when benchmark warmup ends, then embed `report` in its JSON.
#[derive(Resource, Default)]
pub(crate) struct RenderProbe {
    started: Option<Instant>,
    stopped: Option<Instant>,
    series: BTreeMap<String, Samples>,
}

#[derive(Default)]
struct Samples {
    unit: String,
    last_time: Option<Instant>,
    values: Vec<f64>,
}

impl RenderProbe {
    /// Discard startup diagnostics and begin a new observation interval.
    pub fn start(&mut self) {
        self.started = Some(Instant::now());
        self.stopped = None;
        self.series.clear();
    }

    /// Stop collecting without discarding the finished interval.
    pub fn stop(&mut self) {
        if self.started.is_some() && self.stopped.is_none() {
            self.stopped = Some(Instant::now());
        }
    }

    /// Observe each new timestamp exactly once, including any unread history.
    pub fn sample(&mut self, diagnostics: &DiagnosticsStore) {
        let Some(started) = self.started.filter(|_| self.stopped.is_none()) else {
            return;
        };
        for diagnostic in diagnostics.iter() {
            let path = diagnostic.path().as_str();
            if !path.starts_with("render/") && !path.starts_with("mesh_allocator_") {
                continue;
            }
            let samples = self
                .series
                .entry(path.to_owned())
                .or_insert_with(|| Samples {
                    unit: diagnostic.suffix.trim().to_owned(),
                    ..default()
                });
            let after = samples.last_time.unwrap_or(started);
            for measurement in diagnostic.measurements().filter(|m| m.time > after) {
                samples.last_time = Some(measurement.time);
                if measurement.value.is_finite() {
                    samples.values.push(measurement.value);
                }
            }
        }
    }

    /// Per-path distributions retain their source units; timing paths are ms.
    pub fn report(&self) -> serde_json::Value {
        let paths: BTreeMap<_, _> = self
            .series
            .iter()
            .filter(|(_, samples)| !samples.values.is_empty())
            .map(|(path, samples)| (path, samples.report()))
            .collect();
        let elapsed = self.started.map(|start| {
            self.stopped
                .unwrap_or_else(Instant::now)
                .duration_since(start)
                .as_secs_f64()
        });
        serde_json::json!({
            "enabled": true,
            "observation_seconds": elapsed,
            "gpu_timestamps_observed": paths.keys().any(|path| path.ends_with("/elapsed_gpu")),
            "asynchronous_readback": true,
            "note": "Per-pass spans may overlap; their sum is not frame time. GPU observations arrive after the rendered frame and may omit frames.",
            "diagnostics": paths,
        })
    }
}

impl Samples {
    fn report(&self) -> serde_json::Value {
        let mut sorted = self.values.clone();
        sorted.sort_unstable_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            sorted
                .get((sorted.len().saturating_sub(1) as f64 * fraction) as usize)
                .copied()
        };
        let max_increase = self
            .values
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .fold(0.0, f64::max);
        serde_json::json!({
            "unit": self.unit, "samples": sorted.len(),
            "mean": self.values.iter().sum::<f64>() / self.values.len().max(1) as f64,
            "p50": percentile(0.5), "p95": percentile(0.95), "p99": percentile(0.99),
            "min": sorted.first(), "max": sorted.last(), "last": self.values.last(),
            "max_increase": max_increase,
        })
    }
}

fn collect(diagnostics: Res<DiagnosticsStore>, mut probe: ResMut<RenderProbe>) {
    probe.sample(&diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::diagnostic::{Diagnostic, DiagnosticMeasurement, DiagnosticPath};
    use std::time::Duration;

    #[test]
    fn excludes_warmup_deduplicates_readbacks_and_retains_spike_values() {
        let mut probe = RenderProbe::default();
        probe.start();
        let started = probe.started.unwrap();
        let path = DiagnosticPath::new("render/main_pass/elapsed_gpu");
        let mut diagnostic = Diagnostic::new(path.clone()).with_suffix("ms");
        for (seconds, value) in [(0, 500.0), (1, 2.0), (2, 90.0), (3, f64::NAN)] {
            diagnostic.add_measurement(DiagnosticMeasurement {
                time: started + Duration::from_secs(seconds),
                value,
            });
        }
        let mut store = DiagnosticsStore::default();
        store.add(diagnostic);
        probe.sample(&store);
        probe.sample(&store);
        let report = probe.report();
        let measured = &report["diagnostics"][path.as_str()];
        assert_eq!(measured["samples"], 2);
        assert_eq!(measured["max"], 90.0);
        assert_eq!(measured["mean"], 46.0);
        assert_eq!(measured["max_increase"], 88.0);
        assert_eq!(report["gpu_timestamps_observed"], true);
        probe.stop();
        store
            .get_mut(&path)
            .unwrap()
            .add_measurement(DiagnosticMeasurement {
                time: started + Duration::from_secs(4),
                value: 1000.0,
            });
        probe.sample(&store);
        assert_eq!(probe.report()["diagnostics"][path.as_str()]["samples"], 2);
    }
}
