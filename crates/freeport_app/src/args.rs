//! What the command line asked for, and nothing else.
//!
//! The defaults are `main.rs`'s own constants, so a flag and the number it
//! overrides are never written twice.

use crate::{FPS, HEX_SPAN, LEVELS, OCTAVES};
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
    /// How many tiles the hex disc reaches, and how many octaves of
    /// relief the field carries: both are what a picture on a software
    /// rasteriser is bought down with, since the field in the vertex
    /// stage is nearly all of a frame there.
    pub(crate) span: u32,
    pub(crate) octaves: u32,
    /// Draw the hex tiers: a disc of Goldberg columns round the eye and
    /// Planet-LOD past it, both made in the vertex stage. It is the
    /// DEFAULT, because the hex world is what this harness is; `--chunks`
    /// is how the dual contoured one is asked for.
    pub(crate) tiers: bool,
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
        tiers: true,
        fps: FPS,
        span: HEX_SPAN,
        octaves: OCTAVES,
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
            // The tiers are flown over: the walker stands on the dual
            // contoured field, and a hex column's ground is a question
            // `freeport_core::walker` has not been asked yet.
            "--tiers" => args.tiers = true,
            // The dual contoured world: chunks, a sea of its own, the
            // towns and the builder, and the walker on foot in them.
            "--chunks" => args.tiers = false,
            "--fps" => args.fps = it.next().and_then(|v| v.parse().ok()).unwrap_or(FPS),
            "--span" => {
                args.span = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(HEX_SPAN)
                    .clamp(1, 512)
            }
            "--octaves" => {
                args.octaves = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(OCTAVES)
                    .clamp(1, 24)
            }
            other => warn!("unknown argument {other}"),
        }
    }
    // The hex world is flown over: a hex column's ground is a question
    // `freeport_core::walker` has not been asked yet, so there is nothing
    // for the walker to stand on that agrees with the picture.
    if args.tiers {
        args.fly = true;
    }
    args
}
