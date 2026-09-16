//! Repeatable moving-flight measurements, including render/present frame time.

use crate::planets::Planets;
use crate::stream::Streamer;
use crate::{Args, Eye, FlightSettings, Fly, Status};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::pos::WorldPos;
use std::time::Instant;

#[derive(Resource, Default)]
pub(crate) struct Benchmark {
    warmup: u32,
    steps: u32,
    from: Option<DVec3>,
    last: Option<Instant>,
    drive_done: Option<Instant>,
    update_started: Option<Instant>,
    frames: Vec<f64>,
    flight: Vec<f64>,
    stream: Vec<f64>,
    update: Vec<f64>,
    distance: f64,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FlightWorld<'w> {
    planets: Res<'w, Planets>,
    settings: Res<'w, FlightSettings>,
    streamer: Res<'w, Streamer>,
    eye: ResMut<'w, Eye>,
    status: ResMut<'w, Status>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ReportWorld<'w> {
    streamer: Res<'w, Streamer>,
    eye: Res<'w, Eye>,
    tuning: Res<'w, crate::tuning::Tuning>,
    adapter: Res<'w, bevy::render::renderer::RenderAdapterInfo>,
}

pub(crate) fn setup(
    args: Res<Args>,
    planets: Res<Planets>,
    mut eye: ResMut<Eye>,
    mut camera: Query<&mut Fly>,
) {
    if args.benchmark.is_none() {
        return;
    }
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    let body = &planets.bodies[planets.active];
    let up = (camera.at - body.centre).normalize_or(DVec3::Y);
    camera.at += up * args.bench_height;
    camera.speed = args.bench_speed;
    eye.0 = WorldPos(camera.at);
}

/// Benchmarks own their input; typing in another application cannot alter a run.
pub(crate) fn clear_input(
    args: Res<Args>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<Messages<bevy::input::mouse::MouseMotion>>,
) {
    if args.benchmark.is_some() {
        keys.reset_all();
        mouse.clear();
    }
}

pub(crate) fn drive(
    args: Res<Args>,
    mut bench: ResMut<Benchmark>,
    mut world: FlightWorld,
    mut camera: Query<&mut Fly>,
    mut probe: Option<ResMut<crate::render_probe::RenderProbe>>,
) {
    if args.benchmark.is_none() {
        return;
    }
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    bench.warmup += 1;
    if bench.from.is_none() {
        if bench.warmup < 120 || !world.streamer.idle() {
            return;
        }
        bench.from = Some(camera.at);
        if let Some(probe) = probe.as_deref_mut() {
            probe.start();
        }
        info!(
            "flight benchmark begins: {} frames, cruise {} m/s, height offset {} m",
            args.bench_frames, args.bench_speed, args.bench_height
        );
    }
    let now = Instant::now();
    bench.update_started = Some(now);
    if let Some(last) = bench.last.replace(now) {
        bench.frames.push((now - last).as_secs_f64() * 1000.0);
    }
    if bench.steps >= args.bench_frames {
        return;
    }
    let body = &world.planets.bodies[world.planets.nearest(camera.at)];
    let up = (camera.at - body.centre).normalize_or(DVec3::Y);
    let facing = camera.forward().as_dvec3();
    let direction = (facing - up * facing.dot(up)).normalize_or(DVec3::X);
    let started = Instant::now();
    let (at, _) = world.planets.advance(
        camera.at,
        direction,
        args.bench_speed,
        world.settings.surface_speed,
        world.settings.ground_clearance,
        1.0 / 60.0,
    );
    bench.flight.push(started.elapsed().as_secs_f64() * 1000.0);
    bench.distance += (at - camera.at).length();
    camera.at = at;
    // Keep the view tangent as visibility and terrain demand move with the eye.
    camera.face(direction, up);
    bench.steps += 1;
    world.eye.0 = WorldPos(at);
    world.status.walker = format!(
        "flight benchmark {}/{} | travelled {:.1} m",
        bench.steps, args.bench_frames, bench.distance
    );
    bench.drive_done = Some(Instant::now());
}

pub(crate) fn after_stream(mut bench: ResMut<Benchmark>) {
    if let Some(started) = bench.drive_done.take() {
        bench.stream.push(started.elapsed().as_secs_f64() * 1000.0);
    }
}

pub(crate) fn after_update(mut bench: ResMut<Benchmark>) {
    if let Some(started) = bench.update_started.take() {
        bench.update.push(started.elapsed().as_secs_f64() * 1000.0);
    }
}

pub(crate) fn finish(
    args: Res<Args>,
    bench: Res<Benchmark>,
    world: ReportWorld,
    mut exit: MessageWriter<AppExit>,
    mut probe: Option<ResMut<crate::render_probe::RenderProbe>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(path) = &args.benchmark else {
        return;
    };
    if bench.frames.len() < args.bench_frames as usize {
        return;
    }
    let render = probe.as_deref_mut().map(|probe| {
        probe.sample(&diagnostics);
        probe.stop();
        probe.report()
    });
    let report = serde_json::json!({
        "frames": distribution(&bench.frames), "flight_cpu": distribution(&bench.flight),
        "main_update_cpu": distribution(&bench.update), "render": render,
        "stream_and_rebase_cpu": distribution(&bench.stream), "terrain": world.streamer.measurement(),
        "requested_speed_mps": args.bench_speed, "height_offset_m": args.bench_height,
        "travelled_m": bench.distance, "end": world.eye.0.0.to_array(),
        "simulation_hz": 60, "startup_excluded": true,
        "presentation": "AutoNoVsync", "window_updates": "continuous",
        "cell_size_m": args.cell_size.unwrap_or(world.tuning.terrain_cell_size),
        "levels": args.levels, "compute_batch": world.tuning.terrain_compute_batch,
        "requested_cpu_sampling": args.cpu_terrain,
        "gpu": world.adapter.name, "backend": format!("{:?}", world.adapter.backend),
        "resolution": windows.single().ok().map(|w| [w.physical_width(), w.physical_height()]),
        "window_focused_at_finish": windows.single().ok().map(|w| w.focused),
    });
    match serde_json::to_vec_pretty(&report)
        .ok()
        .and_then(|bytes| std::fs::write(path, bytes).ok())
    {
        Some(()) => info!("flight benchmark saved to {path}: {}", report["frames"]),
        None => error!("could not write flight benchmark {path}"),
    }
    exit.write(AppExit::Success);
}

fn distribution(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        sorted
            .get(((sorted.len().saturating_sub(1)) as f64 * p) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    serde_json::json!({"samples": sorted.len(), "p50_ms": percentile(0.5), "p95_ms": percentile(0.95),
        "p99_ms": percentile(0.99), "max_ms": sorted.last(),
        "over_16_67_ms": sorted.iter().filter(|&&v| v > 1000.0 / 60.0).count(),
        "over_33_33_ms": sorted.iter().filter(|&&v| v > 1000.0 / 30.0).count()})
}
