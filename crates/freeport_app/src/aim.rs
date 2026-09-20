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
use crate::roads;
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
    if let Some(up) = args.shore {
        match waterline(world, up) {
            Some(pair) => return pair,
            None => bevy::log::warn!("no shore within reach of the port to aim at"),
        }
    }
    // Straight down on the JUNCTION, which is where the highway's own
    // tarmac meets the port's paving. `--over` looks down at a town's
    // MIDDLE, so the join is out at the edge of its frame and 25 px of
    // it; solved off `roads::mouth_of`, the same point the harness
    // measures the gap at, a camera here frames the join and nothing
    // else. A camera aimed at a junction the harness measures somewhere
    // else is a picture of the wrong place.
    if let Some(up) = args.junction {
        match roads::mouth_of(world) {
            Some((road, town, at, h)) => {
                let ground = at * (world.planet.radius + h);
                bevy::log::info!(
                    "the junction camera stands {up:.0} m over the middle of road {road}'s slip, \
                     {:.0} m out of town {town}, looking straight down",
                    at.angle_between(world.towns[town].dir) * world.planet.radius
                );
                return (ground + at * up, ground);
            }
            None => bevy::log::warn!("no junction to aim at"),
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
    // And the OTHER end of it: out in the country, looking back down the
    // tarmac at the town it runs into. `--road` shows what a highway
    // between two towns looks like and this shows it ARRIVING, which is
    // a different picture and the one the owner asked for.
    if let Some(up) = args.approach {
        match road_in(world, up) {
            Some((from, to)) => {
                bevy::log::info!(
                    "the approach camera stands {up:.0} m over the tarmac {:.0} m out of the port, looking back at it",
                    from.angle_between(to) * world.planet.radius
                );
                return (from + from.normalize_or(DVec3::Y) * up, to);
            }
            None => bevy::log::warn!("no road into the port to aim at"),
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
/// Where a camera stands to photograph the SHEET: out on the water off
/// the port's own shore, `up` metres over the sea, looking out to it.
///
/// Its own function rather than an arm of `aim`, because `aim` is a
/// chain of camera cases and each one that grows takes the whole of it
/// over this project's hundred line limit. The list `shape.py` prints is
/// work and never a reason to raise the limit.
fn waterline(world: &World, up: f64) -> Option<(DVec3, DVec3)> {
    let (at, out) = shore(world)?;
    let sea = world.sea.radius;
    // OUT on the water and not at the last dry step. The waterline is on
    // the BEACH, so an eye a fraction of a metre over it is an eye under
    // the beach's own crest and the frame comes back as sand: measured
    // at 0.4 m, the lower two thirds of it is dune. `SHORE_OUT` puts the
    // eye where a wader stands, which is where the owner's own picture
    // of this was taken from.
    let wet = (at + out * (SHORE_OUT / world.planet.radius)).normalize();
    let eye = wet * (sea + up);
    bevy::log::info!(
        "the shore camera stands {up:.1} m over the sea, {SHORE_OUT:.0} m out from a waterline {:.0} m from the port",
        at.angle_between(world.towns[0].dir) * world.planet.radius
    );
    Some((eye, eye + out * 1_000.0))
}

/// How far out to look for the sea, and how finely. Three kilometres in
/// thirty metre steps: the port stands on a shore by construction
/// (`town::coastal` is why it is the biggest settlement on the body), so
/// the water is near, and thirty metres is well under a beach's own
/// width.
const SHORE_REACH: f64 = 3_000.0;
const SHORE_STEP: f64 = 30.0;
/// How far PAST the waterline the eye stands, metres: out on the water,
/// which is where a picture of the sheet is taken from.
const SHORE_OUT: f64 = 60.0;

/// The port's own WATERLINE and which way the open sea is: the nearest
/// point on any bearing where the ground falls under the sea.
///
/// Scanned over bearings rather than marched down the steepest fall,
/// because a town levels its own site and the fall out of the middle of
/// one says nothing about where the coast is. The NEAREST crossing over
/// all of them is the shore this port actually stands on.
pub(crate) fn shore(world: &World) -> Option<(DVec3, DVec3)> {
    const BEARINGS: usize = 32;
    let town = world.towns.first()?;
    let radius = world.planet.radius;
    let sea = world.sea.radius;
    let (east, north) = town::frame_at(town.dir);
    // The bearing with the most OPEN SEA on it, and not the nearest
    // water. The first cut took the nearest crossing, which on this port
    // is an inlet with a spit across it: the frame came back mostly sand
    // and sky, the water in it measured 0.495 of high frequency against
    // the 2.4 of the sand beside it, and a picture that cannot show the
    // sheet cannot be used to judge it. What this camera is FOR is the
    // sheet, so the bearing is the one whose water runs furthest without
    // land beyond it.
    let mut best: Option<(f64, DVec3, DVec3)> = None;
    for b in 0..BEARINGS {
        let a = b as f64 / BEARINGS as f64 * std::f64::consts::TAU;
        let way = east * a.cos() + north * a.sin();
        let mut dry = town.dir;
        let mut at: Option<DVec3> = None;
        let mut wet = 0.0;
        let mut out = SHORE_STEP;
        while out < SHORE_REACH {
            let d = (town.dir + way * (out / radius)).normalize();
            let under = town::surface_radius(&world.planet, d) < sea;
            match (at.is_some(), under) {
                // The last DRY step is the waterline, to a step.
                (false, true) => at = Some(dry),
                (true, true) => wet += SHORE_STEP,
                // Land again: this bearing is an inlet, not open sea.
                (true, false) => break,
                (false, false) => dry = d,
            }
            out += SHORE_STEP;
        }
        if let Some(at) = at {
            if best.as_ref().is_none_or(|(w, _, _)| wet > *w) {
                best = Some((wet, at, way));
            }
        }
    }
    let (open, at, way) = best?;
    bevy::log::info!("the open sea on that bearing runs {open:.0} m before land");
    // Out to SEA and level with the horizon: the sheet is what this is
    // a picture of, so the eye looks along the water rather than down at
    // it or up into the sky.
    Some((at, (way - at * way.dot(at)).normalize_or(way)))
}

/// How far BACK an approach camera stands, as a multiple of the town's
/// own width.
///
/// Off the TOWN and not off the camera's height, which is what
/// `road_out` solves its look ahead from and is the wrong rule here: a
/// picture of a road LEAVING is framed by how much road is in it, and a
/// picture of one ARRIVING is framed by the place it arrives at. Borrowed
/// from `road_out` the camera stood four kilometres out and the city was
/// a dot at the vanishing point. At two and a half widths a town 483 m
/// across spans about a third of the frame, with a kilometre of its own
/// road running into it.
const STAND_OFF: f64 = 2.5;

/// Where a road ARRIVES at the port, and what it arrives at: a point on
/// its own tarmac out in the country, and the town itself.
///
/// The camera stands at whichever point of the road's own tarmac is
/// nearest `STAND_OFF` town widths out and looks at the town's middle,
/// so the road runs away from the eye and INTO the city rather than out
/// of it. The town's own position and not the near end of the road,
/// because what the picture is of is the road meeting the place.
pub(crate) fn road_in(world: &World, _up: f64) -> Option<(DVec3, DVec3)> {
    let road = world.roads.first()?;
    let route = world.routes.first()?;
    let town = world.towns.get(road.from)?;
    let radius = world.planet.radius;
    let back = town.radius * freeport_core::town::OUTLINE * 2.0 * STAND_OFF;
    // Among the points that actually carry tarmac, the one standing
    // nearest that far out of the town's own middle.
    let at = (0..route.line.len())
        .filter(|i| route.open[*i])
        .min_by(|a, b| {
            let away = |i: usize| (route.line[i].angle_between(town.dir) * radius - back).abs();
            away(*a).total_cmp(&away(*b))
        })?;
    Some((
        route.line[at] * (radius + route.run[at]),
        town.dir * (radius + town.h),
    ))
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
