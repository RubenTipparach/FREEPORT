//! What the command line asked for, and nothing else.
//!
//! The defaults are `main.rs`'s own constants, so a flag and the number it
//! overrides are never written twice.

use crate::{FPS, LEVELS, OCTAVES};
use bevy::math::DVec3;
use bevy::prelude::{warn, Resource};

/// What the command line asked for.
#[derive(Resource, Clone, Debug)]
pub(crate) struct Args {
    pub(crate) wire: bool,
    pub(crate) lod_wire: bool,
    pub(crate) fly: bool,
    pub(crate) eye: Option<DVec3>,
    pub(crate) look: Option<DVec3>,
    /// Stand this many radii off the home body, ALONG the sun, looking at
    /// its centre. A camera for a picture is solved and never hand aimed:
    /// the sun stands over where the world starts, so where it is depends
    /// on where the towns came out, and three runs of this were aimed by
    /// hand at a planet that turned out to be a different one in its own
    /// night.
    pub(crate) sunward: Option<f64>,
    /// Degrees round the body from the sun, for `--sunward`: nought is
    /// the lit face, 180 the body's own midnight and 140 a crescent with
    /// most of the night side in the frame.
    pub(crate) around: f64,
    /// Metres over the PORT to stand and look straight down at it, for a
    /// picture of a town's own plan in the game.
    pub(crate) over: Option<f64>,
    /// What o'clock it is where the world starts, on a twenty four hour
    /// dial: twelve is the sun at its own highest over the port and
    /// nought is its midnight. A day is `day::DAY` long (four hours), so
    /// the clock runs on from here while the picture is taken; over the
    /// thirty frames a headless render takes it moves under a minute of
    /// game time, which is a fiftieth of a degree of sun.
    ///
    /// It is an HOUR and never a sun direction, which is this file's own
    /// solved camera rule arriving at the clock: a direction says nothing
    /// about what time it is anywhere, so a picture asked for at dusk
    /// cannot be aimed by hand. `day::at_oclock` solves the turn.
    pub(crate) hour: Option<f64>,
    /// Metres over the ROAD out of the port, looking along it toward the
    /// town it goes to. A camera for a picture is SOLVED and never hand
    /// aimed: where a road leaves a town depends on where the network
    /// came out, so a hand aimed camera at a road finds a field.
    pub(crate) road: Option<f64>,
    /// Plan the home body's cities and roads, write its atlas and stop.
    /// It runs BEFORE any of Bevy is built, so a bake needs no window, no
    /// device and no Xvfb: it is arithmetic and a file.
    pub(crate) bake_atlas: bool,
    pub(crate) levels: u8,
    pub(crate) shot: Option<String>,
    pub(crate) frames: u32,
    /// Frames a second the loop is held to. Nought lifts it.
    pub(crate) fps: f64,
    /// How many octaves of relief the field carries: what a picture on a
    /// software rasteriser is bought down with, since the field is what a
    /// chunk costs.
    pub(crate) octaves: u32,
    /// Drive the walker FORWARD for this many frames at a fixed sixtieth
    /// of a second. A headless run has nobody to press W, and a walker's
    /// feel is a number a second person can check rather than a thing to
    /// take on trust.
    pub(crate) walk: u32,
    /// STEAL the nearest car and drive it for this many SECONDS, aimed
    /// at the nearest settlement that is not the one it is standing in.
    /// A headless run has nobody to press E and then hold W, and a car
    /// nobody can photograph is a car nobody can judge the feel of. A
    /// second a rendered FRAME, in sixtieths, because a frame of this
    /// world on a software rasteriser is most of a second and the
    /// nearest town is nine kilometres away.
    pub(crate) drive: u32,
    pub(crate) cpu_terrain: bool,
    pub(crate) benchmark: Option<String>,
    pub(crate) bench_frames: u32,
    pub(crate) bench_speed: f64,
    pub(crate) bench_height: f64,
    pub(crate) cell_size: Option<f64>,
    pub(crate) profile_render: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            wire: false,
            lod_wire: false,
            fly: false,
            eye: None,
            look: None,
            sunward: None,
            around: 0.0,
            over: None,
            hour: None,
            road: None,
            bake_atlas: false,
            levels: LEVELS,
            shot: None,
            frames: 30,
            fps: FPS,
            octaves: OCTAVES,
            walk: 0,
            drive: 0,
            cpu_terrain: false,
            benchmark: None,
            bench_frames: 1200,
            bench_speed: 2_000_000.0,
            bench_height: 0.0,
            cell_size: None,
            profile_render: false,
        }
    }
}

pub(crate) fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    let vec3 = |s: &str| -> Option<DVec3> {
        let v: Vec<f64> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3 && v.iter().all(|value| value.is_finite()))
            .then(|| DVec3::new(v[0], v[1], v[2]))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wire" => args.wire = true,
            "--lod-wire" => args.lod_wire = true,
            "--cpu-terrain" => args.cpu_terrain = true,
            "--profile-render" => args.profile_render = true,
            "--fly" => args.fly = true,
            "--sunward" => args.sunward = it.next().and_then(|v| v.parse().ok()),
            "--around" => args.around = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0),
            "--over" => args.over = it.next().and_then(|v| v.parse().ok()),
            "--road" => args.road = it.next().and_then(|v| v.parse().ok()),
            "--hour" => args.hour = it.next().and_then(|v| v.parse().ok()),
            "--bake-atlas" => args.bake_atlas = true,
            "--eye" => args.eye = it.next().and_then(|v| vec3(&v)),
            "--look" => args.look = it.next().and_then(|v| vec3(&v)),
            "--levels" => {
                args.levels = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(LEVELS)
                    .clamp(1, 16)
            }
            "--shot" => args.shot = it.next(),
            "--frames" => args.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(30),
            "--fps" => args.fps = it.next().and_then(|v| v.parse().ok()).unwrap_or(FPS),
            "--octaves" => {
                args.octaves = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(OCTAVES)
                    .clamp(1, 24)
            }
            "--walk" => args.walk = it.next().and_then(|v| v.parse().ok()).unwrap_or(600),
            "--drive" => args.drive = it.next().and_then(|v| v.parse().ok()).unwrap_or(600),
            "--benchmark-flight" => {
                args.benchmark = it.next();
                args.fly = true;
            }
            "--bench-frames" => {
                args.bench_frames = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1200)
                    .max(60)
            }
            "--bench-speed" => {
                args.bench_speed = it
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .unwrap_or(2_000_000.0)
            }
            "--bench-height" => {
                args.bench_height = it
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(0.0)
            }
            "--cell-size" => {
                args.cell_size = it
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && (0.125..=4.0).contains(v))
            }
            other => warn!("unknown argument {other}"),
        }
    }
    args
}
