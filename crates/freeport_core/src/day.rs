//! What time of day it is on a body, and where its sun stands.
//!
//! The body is held STILL and its sky turns, which is what a body fixed
//! frame is. Every direction this game reasons about is written on a
//! sphere that never moves: a town is a direction, a road is a chain of
//! them, a chunk's corner is an index off a lattice pinned to the centre.
//! Spinning the planet would mean moving all of that once a frame and
//! re-meshing nothing, for a picture that is identical to turning the one
//! vector the light, the dome, the fog, the sea and the impostor all read.
//! So the sun goes round and the ground does not.
//!
//! It is in the CORE because what time it is has to be a rule two clients
//! agree on: where a shadow falls, whether a street lamp is lit and which
//! half of a planet is in its own night are facts about the world and not
//! about the renderer.

use crate::noise::smoothstep;
use glam::DVec3;
use std::f64::consts::TAU;

/// How long one full turn of the body takes, seconds. The owner's number:
/// four hours to a day, so an hour of play is a quarter of one and a
/// night is two hours long.
pub const DAY: f64 = 240.0 * 60.0;

/// The body's own polar axis. `biome::Shape::climate` reads latitude
/// straight off `dir.y`, so the poles ARE plus and minus y and a day is a
/// turn about that: the one axis in this crate that already means
/// something, rather than a second one to keep in step with it.
pub const AXIS: DVec3 = DVec3::Y;

/// How far through its day a body is at `seconds`: nought at the start of
/// one and approaching one at the end. Negative time is the day before,
/// so a clock that runs backwards is still a clock.
///
/// ```
/// # use freeport_core::day::{hour, DAY};
/// assert!((hour(DAY * 0.25, DAY) - 0.25).abs() < 1e-12);
/// assert!((hour(DAY * 1.25, DAY) - 0.25).abs() < 1e-12);
/// ```
pub fn hour(seconds: f64, day: f64) -> f64 {
    if !(seconds.is_finite() && day.is_finite() && day > 0.0) {
        return 0.0;
    }
    (seconds / day).rem_euclid(1.0)
}

/// Where the sun stands `seconds` into a day, given where it stood at
/// nought. A turn about `AXIS`, which holds the sun's DECLINATION (its
/// own y, which is the latitude the climate is measured on) and moves
/// only its bearing, so a body keeps its seasons and gains its hours.
///
/// The sign is the one that takes the sun WEST over the ground: east at a
/// direction is `AXIS cross up` (`town::frame_at`), so at a point on plus
/// x the east is minus z, and a sun leaving plus x toward plus z is a sun
/// setting. `the_sun_crosses_the_sky_from_east_to_west` measures it.
pub fn sun_at(noon: DVec3, seconds: f64, day: f64) -> DVec3 {
    let (s, c) = (TAU * hour(seconds, day)).sin_cos();
    DVec3::new(noon.x * c - noon.z * s, noon.y, noon.x * s + noon.z * c).normalize_or(AXIS)
}

/// How high a sun stands over the horizon at a direction, radians:
/// positive is day, nought is the horizon and negative is night.
pub fn elevation(sun: DVec3, dir: DVec3) -> f64 {
    sun.normalize_or_zero()
        .dot(dir.normalize_or(AXIS))
        .clamp(-1.0, 1.0)
        .asin()
}

/// How far the sun has to TURN from where it stands to stand at its
/// highest over `dir`, radians in nought to a turn. The closed form, not
/// a search: the elevation through a day is
/// `A cos(t) + B sin(t) + C` with `A = up.x sun.x + up.z sun.z`,
/// `B = up.z sun.x - up.x sun.z` and `C = up.y sun.y`, which is highest
/// at `atan2(B, A)`.
///
/// This is what makes an HOUR nameable. A sun direction on its own says
/// nothing about what time it is anywhere, so a picture asked for at dawn
/// or at midnight cannot be aimed by hand, which is this project's own
/// camera rule arriving at the clock: solve for the body's own noon at
/// the place the picture is of, and count the hours off that.
pub fn highest(sun: DVec3, dir: DVec3) -> f64 {
    let s = sun.normalize_or_zero();
    let up = dir.normalize_or(AXIS);
    let a = up.x * s.x + up.z * s.z;
    let b = up.z * s.x - up.x * s.z;
    if a == 0.0 && b == 0.0 {
        // The sun is straight up the axis: every hour is the same hour.
        return 0.0;
    }
    b.atan2(a).rem_euclid(TAU)
}

/// What o'clock it is at a place, on a twenty four hour dial, given where
/// the sun stands: twelve when it is highest and nought at its lowest.
/// The dial is the PLAYER's own word for a time and never the length of
/// the day, so a four hour day still has its dawn at six.
///
/// MINUS the turn, because `highest` is how far the sun still has to go:
/// a sun a quarter turn short of its noon is six hours short of it, which
/// is six in the morning and not six in the evening. Written the other
/// way up the clock runs backwards, and every hour in it is still a valid
/// hour, which is what makes the sign worth a test rather than a glance
/// (`the_clock_runs_forwards_and_reads_twelve_at_the_suns_own_noon`).
pub fn oclock(sun: DVec3, dir: DVec3) -> f64 {
    (12.0 - highest(sun, dir) / TAU * 24.0).rem_euclid(24.0)
}

/// The seconds into a day at which it reads `oclock` at `dir`, for a sun
/// that stood at `noon` when the clock read nought. The inverse of
/// `oclock`, and what a flag asking for a picture at an hour is turned
/// into.
pub fn at_oclock(noon: DVec3, dir: DVec3, oclock: f64, day: f64) -> f64 {
    let now = self::oclock(noon, dir);
    ((oclock - now) / 24.0).rem_euclid(1.0) * day
}

/// Where the TERMINATOR falls, as the sine of the sun's elevation over a
/// place's own horizon: full day past the first and full night past the
/// second. A BAND rather than a line because a planet has air, so the sun
/// sets over a few degrees of longitude and a hard edge round a body
/// reads as a seam somebody drew on it.
pub const DUSK_FROM: f64 = 0.14;
pub const DUSK_TO: f64 = -0.10;

/// How much DAYLIGHT a place has: one under a sun well up, nought well
/// after it has set, and the band between.
///
/// This is the reference and there are THREE transcriptions of it, each
/// naming this function in its own comment: `distant.wgsl` fades a body's
/// albedo to its night floor by it and burns the cities on the dark half,
/// `water.wgsl` decides by it whether a sheet mirrors the day sky or the
/// one the dome is painting at night, and `terrain.wgsl` lights the
/// street lamps and the windows of a town by it. `lamps.rs` calls this
/// one directly, so the light a lamp CASTS and the glow the pane is drawn
/// with are one answer rather than two that have to agree. They are
/// transcriptions and not copies because a fragment shader cannot call
/// into this crate; the constants and the shape are here.
pub fn daylight(sun: DVec3, dir: DVec3) -> f64 {
    let up = sun
        .normalize_or_zero()
        .dot(dir.normalize_or(AXIS))
        .clamp(-1.0, 1.0);
    smoothstep(DUSK_TO, DUSK_FROM, up)
}

/// How hard a place's own lamps are burning: the other side of the
/// daylight, so a lamp comes on as the sun goes down and nothing decides
/// it twice.
pub fn lamplight(sun: DVec3, dir: DVec3) -> f64 {
    1.0 - daylight(sun, dir)
}

#[cfg(test)]
mod tests;
