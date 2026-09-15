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
    pub(crate) fly: bool,
    pub(crate) eye: Option<DVec3>,
    pub(crate) look: Option<DVec3>,
    pub(crate) levels: u8,
    pub(crate) shot: Option<String>,
    pub(crate) frames: u32,
    /// A shape the builder places at the crosshair once the first load has
    /// settled, so a headless run can photograph an edit and the chunks it
    /// remade.
    pub(crate) sculpt: Option<String>,
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
}

pub(crate) fn parse_args() -> Args {
    let mut args = Args {
        wire: false,
        fly: false,
        eye: None,
        look: None,
        levels: LEVELS,
        shot: None,
        frames: 30,
        sculpt: None,
        fps: FPS,
        octaves: OCTAVES,
        walk: 0,
    };
    let mut it = std::env::args().skip(1);
    let vec3 = |s: &str| -> Option<DVec3> {
        let v: Vec<f64> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| DVec3::new(v[0], v[1], v[2]))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wire" => args.wire = true,
            "--fly" => args.fly = true,
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
            "--sculpt" => args.sculpt = it.next(),
            "--fps" => args.fps = it.next().and_then(|v| v.parse().ok()).unwrap_or(FPS),
            "--octaves" => {
                args.octaves = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(OCTAVES)
                    .clamp(1, 24)
            }
            "--walk" => args.walk = it.next().and_then(|v| v.parse().ok()).unwrap_or(600),
            other => warn!("unknown argument {other}"),
        }
    }
    args
}
