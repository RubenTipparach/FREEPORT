//! Where a camera STANDS for a picture, and what o'clock it is there.
//!
//! A camera for a picture is SOLVED and never hand aimed, which is
//! tenebris's LODCAM lesson: its yaw and pitch are solved against the
//! live basis every frame after a week of guessed angles staring at
//! empty sky. Everything in this file is that rule at one of the things
//! this world has to be photographed from: a body from orbit, a town
//! from above, a road along itself, and an hour of the day, which is
//! the same problem in time rather than in space.

use crate::args::Args;
use crate::planets;
use crate::sky;
use crate::world::{start, World};
use bevy::math::DVec3;
use freeport_core::day;
use freeport_core::town;

/// Where the sun stands over the WORLD's own starting point: degrees over
/// the local horizon there, and degrees round from local north. A low sun
/// is the light a landscape reads best in, and the bearing puts it off the
/// shoulder rather than behind the camera. It is the world's start and
/// never `--eye`, so two pictures from two places are lit alike.
///
/// It was a fixed world direction, and its own comment claimed it stood
/// "a little over the horizon at the harness's start", which is a thing a
/// world direction cannot promise: it is true of one spot on the planet
/// and the towns are placed by the ground. On this planet the port came
/// out 56 degrees into its own NIGHT, and a picture of a city at midnight
/// is a picture of nothing. `sun_over` measures it from where the world
/// starts instead, so the claim is kept by construction on any planet,
/// any seed and any port.
const SUN_UP: f64 = 32.0;
const SUN_BEARING: f64 = 40.0;

/// The sun's world direction for an eye starting at `dir`: ONE number,
/// read by the light that casts the shadows, by the sky dome, by the fog
/// and by the bodies drawn from far off, so they cannot point four ways.
///
/// It is asked about the WALKER's own start and never about where `aim`
/// put the camera, which is a circle: `--sunward` stands the camera along
/// the sun, so a sun measured over that camera is a sun measured over
/// itself. Measured on this planet, `--sunward 2.6 --around 150` put the
/// camera 58 degrees from the sun rather than 150, and the picture of the
/// body's own midnight came back three quarters lit.
/// Where the camera starts: on a street of the port, or, with
/// `--sunward`, that many radii off the body and looking at its centre,
/// `--around` degrees round from the sun.
///
/// A camera for a picture is SOLVED and never hand aimed, which is
/// tenebris's LODCAM lesson: the sun stands over wherever the world
/// starts, so where it is depends on where the towns came out, and three
/// runs of this were aimed by hand at a planet that turned out to be a
/// different one, in its own night.
///
/// `--around` is the same rule for the NIGHT side. A body's dark half is
/// a picture nobody can aim at either, because where it is depends on
/// where the sun came out: turned 180 degrees the camera is at the body's
/// own midnight and 140 leaves a crescent of day in the frame, which is
/// what shows the lights and the ground they stand on in one picture.
pub(crate) fn aim(world: &World, args: &Args) -> (DVec3, DVec3) {
    let (eye, look) = start(world);
    // Straight down on the PORT, which is the one camera a town's own
    // plan can be judged from: its outline, its zones and where its
    // streets run are a thing seen from above and nothing else.
    if let Some(over) = args.over {
        if let Some(port) = world.towns.first() {
            let ground = port.dir * (world.planet.radius + port.h);
            return (ground + port.dir * over, ground);
        }
    }
    // ALONG the road out of the port, which is the one camera the
    // tarmac between two towns can be judged from. Solved rather than
    // aimed: it stands `--road` metres over the corridor a little way
    // out of town and looks down it toward the town it leads to.
    if let Some(up) = args.road {
        match road_out(world, up) {
            Some((from, to)) => {
                bevy::log::info!(
                    "the road camera stands {up:.0} m over the tarmac out of the port, looking {:.2} km down it",
                    from.angle_between(to) * world.planet.radius / 1000.0
                );
                return (from + from.normalize_or(DVec3::Y) * up, to);
            }
            // NAMED rather than silent: a camera that quietly falls back
            // to the world's own start is a picture of a street with
            // `--road` on the command line, which is what the first two
            // renders of this were.
            None => bevy::log::warn!("no road out of the port to aim at"),
        }
    }
    match args.sunward {
        Some(radii) => (
            turned(sun_over(eye), args.around.to_radians()) * world.planet.radius * radii.max(1.05),
            DVec3::ZERO,
        ),
        None => (eye, look),
    }
}
/// A direction turned `angle` away from itself, about whichever axis is
/// square to it. Which axis does not matter for a picture of a sphere:
/// what is being asked for is how much of the body's night is in frame,
/// and that is the ANGLE alone.
/// Where the road out of the PORT is, and where it goes: a point on its
/// own tarmac a little clear of the town, and a point further along it.
///
/// The first road in the network that starts or ends at town nought,
/// taken at the first point of its centreline that is outside every
/// town's levelling, which is exactly where the tarmac starts.
pub(crate) fn road_out(world: &World, up: f64) -> Option<(DVec3, DVec3)> {
    let (k, road) = world
        .roads
        .iter()
        .enumerate()
        .find(|(_, r)| r.from == 0 || r.to == 0)?;
    let route = world.routes.get(k)?;
    let radius = world.planet.radius;
    let ahead = road.from == 0;
    let at = |i: usize| route.line[i] * (radius + route.run[i]);
    let open: Vec<usize> = route
        .open
        .iter()
        .enumerate()
        .filter(|(_, o)| **o)
        .map(|(i, _)| i)
        .collect();
    let (first, last) = (*open.first()?, *open.last()?);
    // How far down the road to LOOK, METRES. It is a function of how
    // high the camera stands, because the two are one framing: from
    // three metres up a road runs to the horizon and 300 m of it fills
    // the frame, and the first cut of this looked sixty pieces ahead,
    // which is twenty kilometres and well under the horizon of an eye
    // 25 m up. The picture came back as bare hills.
    //
    // In METRES and not in PIECES, which is the dashes' and the lamps'
    // own mistake a third time in the same file: written as a count of
    // pieces it was 341 m of road at the old spacing and 85 at the new,
    // so quartering the piece quartered the framing and the picture came
    // back with the camera's nose on the tarmac.
    const LOOK: f64 = 100.0;
    let piece = freeport_core::road::PIECE;
    let pieces = (((up * LOOK).clamp(piece, 4_000.0) / piece).ceil() as usize).max(1);
    // And OUT of the town first. The first open point is a couple of
    // hundred metres from the town's centre, which is its own edge: the
    // picture from there is a street with a field at the end of it, and
    // what this camera is for is the country road.
    //
    // 341 METRES and not three pieces, which is measured rather than
    // chosen and is in metres for the reason above. At three pieces of
    // 341 the camera stood about 1.7 km from the port's middle looking
    // further out, and `road::LIT_NEAR` is 1.5 km: every lamp on the
    // road was BEHIND it, so the night picture of a lit approach came
    // back with nothing on it at all. 341 m of country between the
    // camera and the town's own edge is still a country road, and it is
    // inside the lighting rather than past it.
    const CLEAR_M: f64 = 341.0;
    let clear = ((CLEAR_M / piece).ceil() as usize).max(1);

    let (start, along) = if ahead {
        (
            (first + clear).min(last),
            (first + clear + pieces).min(last),
        )
    } else {
        (
            last.saturating_sub(clear).max(first),
            last.saturating_sub(clear + pieces).max(first),
        )
    };
    Some((at(start), at(along)))
}
pub(crate) fn turned(dir: DVec3, angle: f64) -> DVec3 {
    let (east, _) = town::frame_at(dir);
    (dir * angle.cos() + east * angle.sin()).normalize_or(DVec3::Y)
}
pub(crate) fn sun_over(dir: DVec3) -> DVec3 {
    let (east, north) = town::frame_at(dir);
    let up = SUN_UP.to_radians();
    let round = SUN_BEARING.to_radians();
    (dir.normalize_or(DVec3::Y) * up.sin() + (north * round.cos() + east * round.sin()) * up.cos())
        .normalize_or(DVec3::Y)
}
/// The weather this world starts under, clock and all. `sun` is where
/// the sun stood when the clock read nought and `here` is where the world
/// starts, so `--hour` is SOLVED back into the seconds that put the hour
/// asked for over that spot: an hour is a thing a person can ask for and
/// a sun direction is not, which is this file's own solved camera rule
/// arriving at the time of day. With no flag the clock starts at nought
/// and the sun is where `sun_over` put it, so every picture taken before
/// there was a day is the picture it always was.
pub(crate) fn clock_of(args: &Args, sun: DVec3, here: DVec3, body: &planets::Body) -> sky::Weather {
    let start = args
        .hour
        .map(|h| day::at_oclock(sun, here, h, day::DAY))
        .unwrap_or(0.0);
    sky::Weather {
        air: body.air,
        sea: body.world.sea.radius,
        sun: day::sun_at(sun, start, day::DAY),
        noon: sun,
        start,
        now: start,
        day: day::DAY,
        here,
    }
}
