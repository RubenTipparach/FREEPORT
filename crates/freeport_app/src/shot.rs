//! The harness's own PICTURE: with `--shot`, the frame the arguments
//! asked for, taken once the ground, the towns and any map over them have
//! settled, and the frame times it took written beside it.

use crate::args::Args;
use crate::stream::Streamer;
use crate::{map, world};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::time::Instant;

/// With `--shot`, save the frame the arguments asked for once the streamer
/// has settled (or ten times as many frames on), and leave a few frames
/// later, once the write has had its chance.
pub(crate) fn take_shot(
    mut commands: Commands,
    args: Res<Args>,
    streamer: Option<Res<Streamer>>,
    fabric: Option<Res<world::Fabric>>,
    map: (Res<map::MapView>, Res<map::Relief>),
    mut shot: Local<ShotState>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &args.shot else {
        return;
    };
    shot.frame += 1;
    let now = Instant::now();
    if let Some(last) = shot.last.replace(now) {
        shot.times.push((now - last).as_secs_f64() * 1000.0);
    }
    // The picture waits for the ground: every chunk the rings want drawn
    // once, or ten times the frames asked for, whichever comes first.
    // And a map open over it waits for its own picture, which is drawn
    // on a thread and lands a few frames after the view is settled.
    // And for the towns: every tile drawn at the grade its distance asks
    // for, or a picture of a street is a picture of solid blocks.
    let late = shot.frame >= args.frames * 10;
    let ready = match &streamer {
        Some(s) => s.idle() || late,
        None => true,
    } && (fabric.as_ref().is_none_or(|f| f.settled()) || late)
        && (map::relief_ready(&map.0, &map.1) || late);
    if shot.taken.is_none() && shot.frame >= args.frames && ready {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        shot.taken = Some(shot.frame);
        shot.save_metrics(path, streamer.as_deref(), fabric.as_deref());
    }
    if shot.taken.is_some_and(|t| shot.frame >= t + 12) {
        exit.write(AppExit::Success);
    }
}

#[derive(Default)]
pub(crate) struct ShotState {
    frame: u32,
    taken: Option<u32>,
    last: Option<Instant>,
    times: Vec<f64>,
}

impl ShotState {
    fn save_metrics(
        &self,
        path: &str,
        streamer: Option<&Streamer>,
        fabric: Option<&world::Fabric>,
    ) {
        let mut times = self.times.clone();
        times.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            times
                .get(((times.len().saturating_sub(1)) as f64 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let value = serde_json::json!({
            "frames": self.frame, "frame_p50_ms": percentile(0.5), "frame_p95_ms": percentile(0.95),
            "frame_max_ms": times.last(), "terrain": streamer.map(Streamer::measurement),
            "towns": fabric.map(|f| f.drawing),
        });
        if let Some(f) = fabric {
            info!("towns drawn at the picture: {:?}", f.drawing);
        }
        let destination = std::path::Path::new(path).with_extension("metrics.json");
        if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
            if let Err(e) = std::fs::write(destination, bytes) {
                warn!("could not write screenshot metrics: {e}");
            }
        }
    }
}
