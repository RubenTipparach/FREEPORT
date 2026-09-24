//! The SCRIPTED drive: what a headless run presses in place of E and W,
//! where it is headed, and how it steers there, which is along the ROUTE
//! the map plans to its goal and through a town's own streets where the
//! tarmac stops.

use super::{Driver, Streets, Thefts, SUB_STEPS};
use crate::route::{ahead_on, bend_limit, off_tarmac, tarmac_after, Look, Planned};
use crate::{Args, Controls};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::driver::{bend_speed, Drive, TOP};
use freeport_core::town::{self, lot_frame};
use freeport_core::{field, road};

/// What the driver asked for this frame, how long the frame is and how
/// many sub steps of it to take: the KEYS, or the scripted drive's own
/// held throttle where one is running.
///
/// Its own function because reading the player and reading the flag are
/// two things, and `drive_car` was over this project's own hundred lines
/// carrying both.
///
/// A scripted drive holds the throttle and steps a fixed sixtieth, which
/// is `--walk`'s own rule and for the same reason: the frame's own delta
/// on a software rasteriser is most of a second and a car that moves
/// nine metres a frame measures nothing. It takes a whole SECOND of
/// driving per rendered frame, because a drive stepped one sixtieth a
/// frame covers 480 m in half an hour of rendering and the nearest
/// settlement is nine kilometres off: the flag could photograph a car
/// and never a JOURNEY.
pub(super) fn pedals(controls: &Controls, script: &Script, auto: &mut Auto) -> (Drive, f64, usize) {
    let keys = &controls.keys;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f64;
    let run = auto.left.get_or_insert(script.args.drive);
    if *run > 0 {
        *run -= 1;
        return (
            Drive {
                throttle: 1.0,
                ..Default::default()
            },
            1.0 / 60.0,
            SUB_STEPS,
        );
    }
    let input = Drive {
        throttle: axis(KeyCode::KeyS, KeyCode::KeyW),
        // A is LEFT, and left is a turn toward the car's own port side,
        // which is the negative of the right hand axis the heading turns
        // about. Reading it the other way steered into the kerb.
        steer: axis(KeyCode::KeyD, KeyCode::KeyA),
        brake: keys.pressed(KeyCode::Space),
    };
    (input, controls.time.delta_secs_f64(), 1)
}

/// How long a scripted car has to have made no ground before it is
/// STUCK, seconds, and how long it then backs off for.
const WEDGED: f64 = 1.5;
const BACK_OFF: f64 = 2.5;
/// How little GROUND it has to have made to count as stuck, metres a
/// second.
///
/// Ground made and not the speedometer. The car that wedged itself out
/// of the port read 4 km/h on its own `speed` while making nought
/// metres a second over the ground for two minutes: a crash scales the
/// speed down rather than stopping the car, so the number on the dial
/// is what the engine is asking for and not what the wheels are doing.
/// A rule that reads the dial never fires.
const CRAWL: f64 = 1.0;
/// What a scripted car plans its braking on, metres a second a second:
/// under the car's own brake, so it arrives at a bend slow rather than
/// late.
const BRAKING: f64 = 8.0;
/// How far past its own braking distance it reads the route's bends,
/// metres.
const PREVIEW: f64 = 30.0;
/// The slowest it slows to, metres a second: a walking pace round the
/// tightest corner, and over `CRAWL`, so braking for a corner is never
/// taken for being stuck.
const CREEP: f64 = 2.0;
/// How far over what a bend allows it lets the car run before it
/// brakes, metres a second, so it is not on and off the pedals every
/// sub step.
const SLACK: f64 = 1.0;

/// What a SCRIPTED drive is doing: how much of it is left to run, and
/// what the car is doing about whatever it has driven into.
///
/// It steers STRAIGHT AT where it is going, which on a street between
/// two buildings is straight at a wall: the first drive to the next
/// town wedged the car against a corner of the port and held the
/// throttle on it for eight hundred seconds, closing ten metres of nine
/// kilometres. A player steers round a building and a script has to be
/// told to.
///
/// This is not path finding and is not pretending to be. It is what a
/// driver does when the nose is against something: back off, turn, and
/// try again. The way it turns is the way the goal is, so it works its
/// way round the obstruction rather than oscillating on one side of it.
#[derive(Default)]
pub struct Auto {
    /// How many seconds of the scripted drive are left to run.
    left: Option<u32>,
    /// How long it has been going nowhere, seconds.
    wedged: f64,
    /// How much of the backing off is left, seconds.
    backing: f64,
    /// Where the car was on the previous sub step, so how far it has
    /// actually come over the ground can be measured.
    was: Option<DVec3>,
    /// How far it has come over the ground, metres, and for how many
    /// scripted seconds, so a drive says whether it is a JOURNEY.
    gone: f64,
    ticks: u32,
    /// The streets of the town it is getting OUT of, and which town that
    /// is. Kept because the graph is a sort of a thousand edges and the
    /// answer only changes when the car leaves the town; dropped by
    /// being replaced when it does.
    streets: Option<(usize, Streets)>,
    /// WHICH of the three things is steering, and how far off its aim
    /// is. A stuck car is a car aiming at something, and a report that
    /// says only how far it has come cannot tell a car following a road
    /// into a hill from a car turning circles on its own bearing.
    why: (&'static str, f64),
    /// How far off the route's tarmac the car stands, metres, which is
    /// what says whether it is ON the route or finding its way onto it.
    off: f64,
    /// The fastest the route's own bends ahead let the car go, metres a
    /// second, and nothing where it is not on the route.
    limit: Option<f64>,
    /// Whether the car has reached the route's tarmac, and not strayed
    /// `STRAYED` off it since. See `along`.
    joined: bool,
}

impl Auto {
    /// How far a scripted drive has come, every half minute of it.
    ///
    /// A drive between two towns is a JOURNEY or it is not, and the one
    /// number that says which is the GROUND made. The speedometer is not
    /// that number: the car that wedged itself out of the port read
    /// 4 km/h while making nought metres for two minutes.
    pub(super) fn say(&mut self, car: &Driver, was: DVec3, goal: Option<DVec3>, radius: f64) {
        self.gone += was.angle_between(car.dir) * radius;
        self.ticks += 1;
        if !self.ticks.is_multiple_of(30) {
            return;
        }
        info!(
            "driven {:.0} m in {} s at {:.0} km/h, {:.2} km to go{}",
            self.gone,
            self.ticks,
            car.speed.abs() * 3.6,
            goal.map_or(0.0, |g| car.dir.angle_between(g) * radius / 1000.0),
            if self.backing > 0.0 {
                ", backing off"
            } else {
                ""
            },
        );
        info!(
            "  steering for {} {:.0} m off, {:.1} m off the route's tarmac",
            self.why.0, self.why.1, self.off
        );
    }

    /// Where a scripted drive STEERS FOR: along the ROUTE the map
    /// planned to its goal, and where there is none, the road when it is
    /// on one, the way out of the town along that town's own streets
    /// when it is in one, and the town it is driving to when it is
    /// neither.
    ///
    /// The route is what makes the goal REACHABLE. Without it the car
    /// followed ONE road, the one it joined, so a settlement that was
    /// not on that road was a settlement it drove past rather than to,
    /// and the distance to the goal never closed.
    pub(super) fn aim(
        &mut self,
        script: &Script,
        here: &crate::world::Surface,
        car: &Driver,
    ) -> Option<DVec3> {
        let world = here.world();
        let (goal, radius) = (script.goal.0?, world.planet.radius);
        self.limit = None;
        let (what, at) = match self.along(script, world, car) {
            Some(found) => found,
            None => self.alone(script, world, car, goal),
        };
        let aim = at.normalize();
        self.why = (what, car.dir.angle_between(aim) * radius);
        Some(aim)
    }

    /// Along the ROUTE: `AHEAD` on along its tarmac while the car is on
    /// it, and where the tarmac stops (the way on out of a town, the way
    /// across one between two roads, a village the highway stops short
    /// of) through that town's own streets to where the tarmac starts
    /// again. Nothing when there is no route over the roads to follow.
    fn along(
        &mut self,
        script: &Script,
        world: &crate::world::World,
        car: &Driver,
    ) -> Option<(&'static str, DVec3)> {
        let leg = script.route.first().filter(|l| l.roads)?;
        let (radius, points) = (world.planet.radius, &leg.points);
        let (seg, off, foot) = off_tarmac(points, car.dir, radius)?;
        self.off = off;
        // JOINED is a latch. The streets are the way ONTO the route from
        // inside a town, and never the way back to it once the car is on
        // it: a car a few metres wide of its line at speed, still inside
        // the port's reach, was handed to the streets, which took it back
        // into town, down the slip and off the far end again, round the
        // junction at 160 km/h for twenty seconds until it wedged against
        // a building. Joined, it pursues the tarmac until it has truly
        // left it.
        if off <= ONTO {
            self.joined = true;
        } else if off > STRAYED {
            self.joined = false;
        }
        // OFF the tarmac inside a town, the way ONTO it is the town's own
        // streets, to the nearest point of it. The route's first step is
        // a hop from where it was planned onto the road beside it, and
        // driven straight that is straight across whatever stands between:
        // the drive out of the port held its throttle against a building
        // eighteen metres from the slip for six minutes. Out in the
        // country there is no town, and the tarmac is pursued from where
        // the car stands.
        if !self.joined {
            if let Some(p) = self.through_town(world, car.dir * car.foot, foot * car.foot) {
                return Some(("the streets onto the route", p));
            }
        }
        let look = Look {
            ahead: AHEAD,
            short: SHORT_HOP,
            lane: freeport_core::road::ribbon::HALF,
        };
        // Along from the tarmac the car is ON, and never from the nearest
        // point of the leg, which may be the hop's own start: from
        // whichever END of that segment the car is nearer, so a car in the
        // last half of the tarmac before a hop takes the hop.
        let d = |k: usize| points[k].0.angle_between(car.dir);
        let near = if d(seg + 1) < d(seg) { seg + 1 } else { seg };
        let reach = car.speed * car.speed / (2.0 * BRAKING) + PREVIEW;
        self.limit = Some(bend_limit(points, near, car.dir, BRAKING, reach, radius));
        let (k, hop) = ahead_on(points, car.dir, near, &look, radius);
        if k > near {
            return Some(("the route", points[k].0));
        }
        if !hop {
            // Nearer the end than anything else on it: the car is there.
            return Some(("the goal", points[points.len() - 1].0));
        }
        let next = points[tarmac_after(points, k)].0;
        Some(
            match self.through_town(world, car.dir * car.foot, next * car.foot) {
                Some(p) => ("the streets", p),
                None => ("the route's tarmac", next),
            },
        )
    }

    /// The answer with no route: the road when the car is on one, the
    /// way out of the town along that town's own streets when it is in
    /// one, and the town it is driving to when it is neither.
    ///
    /// Three answers and not two, because a car stolen in the middle of
    /// the port is 700 m from the nearest tarmac, which is further than
    /// `OFF_ROAD`: `Network::follow` gives up there and the drive aimed
    /// at a settlement nine kilometres off, which from a street between
    /// two buildings is aiming at a wall. The streets are the way out and
    /// `Streets::route` walks them.
    fn alone(
        &mut self,
        script: &Script,
        world: &crate::world::World,
        car: &Driver,
        goal: DVec3,
    ) -> (&'static str, DVec3) {
        let radius = world.planet.radius;
        match script.roads.follow(world, car.dir, goal, AHEAD, radius) {
            Some(p) => ("the road", p),
            None => match script.roads.mouth(world, car.dir) {
                None => ("its town", goal),
                Some(mouth) => match self.through_town(world, car.dir * car.foot, mouth) {
                    Some(p) => ("the streets", p),
                    None => ("the tarmac", mouth),
                },
            },
        }
    }

    /// The next crossing along the streets of whatever town the car is
    /// at, toward a place: the one after the nearest when the car is ON
    /// the town's paving, and the nearest itself when it is arriving
    /// from outside. Nothing when it is at no town, or when the paving
    /// never joined the two.
    ///
    /// AT a town reaches as far as the town's own tarmac does, which is
    /// its levelling and the skirt round it (`road::open`), and a piece
    /// past that: a highway stops short of every village it passes
    /// through, and a car at the end of that tarmac is outside the
    /// village's own outline with nothing but the village between it
    /// and where the road starts again.
    fn through_town(
        &mut self,
        world: &crate::world::World,
        at: DVec3,
        mouth: DVec3,
    ) -> Option<DVec3> {
        let radius = world.planet.radius;
        let dir = at.normalize_or(DVec3::Y);
        let (k, town) = world.towns.iter().enumerate().find(|(_, t)| {
            let away = t.dir.angle_between(dir) * radius;
            away < t.radius * town::OUTLINE * 2.0 + road::PIECE && {
                let site = town::site_of(t);
                away <= site.level_r(dir) + field::site_skirt(&site) + road::PIECE
            }
        })?;
        if self.streets.as_ref().is_none_or(|(i, _)| *i != k) {
            self.streets = Some((k, Streets::of(town)));
        }
        let streets = &self.streets.as_ref()?.1;
        let frame = lot_frame(radius, town, 0.0, 0.0);
        let flat = |p: DVec3| {
            let l = frame.local(p);
            bevy::math::DVec2::new(l.x, l.y)
        };
        let (from, to) = (flat(at), flat(mouth));
        let route = streets.route(from, to);
        // Arriving from OUTSIDE the paving, the nearest crossing is the
        // way in, and the one after it is across whatever stands between.
        let first = route.first()?;
        if (from - *first).length() > town::PITCH * ENTER {
            return Some(frame.world(DVec3::new(first.x, first.y, 0.0)));
        }
        // The NEXT crossing, and never one beyond it. A route is a chain
        // of crossings joined by streets the town laid, so the straight
        // line to the next one is on the paving and the straight line to
        // the one after it is through whatever stands on the corner: at
        // a look ahead of thirty metres on a pitch of eighteen and a
        // half, every route skipped at least one node, and the drive out
        // of the port made 230 m and then held the throttle against a
        // building with its aim 33 m away for the rest of the run. There
        // is nothing to tune here, which is why the constant went: a
        // town's look ahead IS its pitch.
        //
        // None when there is no next one, which means the car is at the
        // town's own exit; what it wants then is the tarmac.
        let next = route.get(1)?;
        Some(frame.world(DVec3::new(next.x, next.y, 0.0)))
    }

    /// The pedals and the wheel for one sub step.
    pub(super) fn drive(
        &mut self,
        car: &Driver,
        goal: Option<DVec3>,
        dt: f64,
        radius: f64,
    ) -> Drive {
        let want = goal.map_or(0.0, |g| car.toward(g));
        let made = self.was.map_or(0.0, |w| w.angle_between(car.dir) * radius);
        self.was = Some(car.dir);
        if self.backing > 0.0 {
            self.backing -= dt;
            // Backing out with the wheel the OTHER way, so the nose
            // swings toward the goal as the car comes off whatever it
            // met. A car reverses along the arc its front wheels cut,
            // so the same lock backwards turns it the other way.
            return Drive {
                throttle: -1.0,
                steer: -want.signum(),
                brake: false,
            };
        }
        if made < CRAWL * dt {
            self.wedged += dt;
        } else {
            self.wedged = 0.0;
        }
        if self.wedged > WEDGED {
            self.wedged = 0.0;
            self.backing = BACK_OFF;
        }
        let (throttle, brake) = self.pace(car, goal, radius);
        Drive {
            throttle,
            steer: want,
            brake,
        }
    }

    /// The throttle or the BRAKE, whichever takes the car to the speed it
    /// can hold the bend it is steering through at, and every bend of
    /// the route ahead of it.
    ///
    /// The bend it is steering through is the pure pursuit arc to its own
    /// aim, `2 sin(off) / reach`, which is a turn the car has to be able
    /// to hold or it circles the point instead of reaching it. It held
    /// 160 km/h into the port's slip, whose turns are fifteen metres,
    /// ran wide onto the grass a hundred metres off its route and went
    /// round and round there for two minutes at the same 51.40 km from
    /// its goal. Never under `CREEP`, or a car braking for a corner reads
    /// as a car wedged against a wall and backs off it.
    fn pace(&self, car: &Driver, goal: Option<DVec3>, radius: f64) -> (f64, bool) {
        let mut most = self.limit.unwrap_or(TOP);
        if let Some(g) = goal {
            let to = (g - car.dir * g.dot(car.dir)).normalize_or_zero();
            let off = to.dot(-car.right()).atan2(to.dot(car.fwd)).abs();
            let reach = (car.dir.angle_between(g) * radius).max(1.0);
            let arc = 2.0 * off.min(std::f64::consts::FRAC_PI_2).sin() / reach;
            most = most.min(bend_speed(arc));
        }
        let most = most.max(CREEP);
        if car.speed > most + SLACK {
            (0.0, true)
        } else if car.speed < most {
            (1.0, false)
        } else {
            (0.0, false)
        }
    }
}

/// What a SCRIPTED drive is: the flags it was given and where it is
/// headed. One thing, because they are one question and asking it as
/// two took `drive_car` over Bevy's own parameter limit.
#[derive(SystemParam)]
pub struct Script<'w> {
    pub(super) args: Res<'w, Args>,
    pub(super) goal: Res<'w, Goal>,
    /// The tarmac, so a scripted drive FOLLOWS the road to its town
    /// rather than aiming through whatever stands between.
    roads: Res<'w, crate::roads::Network>,
    /// The ROUTE to the goal over the roads, which the drive marks on
    /// the map and then drives, and the markers it was planned through.
    route: Planned<'w>,
}

/// How far down the road a scripted drive looks, metres: far enough that
/// the wheel is not sawed at and near enough that a bend is taken rather
/// than cut. Pure pursuit's own one knob.
const AHEAD: f64 = 400.0;

/// The longest hop in a route a car simply drives across, metres: the
/// step from where the route was planned onto the road beside it, or a
/// fork where two roads' lines part inside one corridor. A hop longer
/// than that is a town or a village, and those are driven on their own
/// streets. Shorter than a piece, so a village's closed stations, a
/// piece apart, are never mistaken for one.
const SHORT_HOP: f64 = 50.0;

/// How far off its route a car may stand and still be ON it, metres: the
/// highway's own carriageway and shoulder either side of the centreline,
/// which a car in its lane is well inside.
const ONTO: f64 = road::ribbon::HALF + road::ribbon::SHOULDER;

/// How far off the route's tarmac a car that has JOINED it has to stray
/// before the streets are asked for the way back, metres: a block and a
/// street, the width of a town's own grid, so a car that has run wide
/// of a bend pursues the tarmac and one that has left it for a street
/// takes the streets.
const STRAYED: f64 = 60.0;

/// How far off the nearest crossing a car has to stand, in pitches of
/// the town's grid, to be ARRIVING at a town rather than on its paving:
/// a car anywhere on a street is within half a pitch of a crossing.
const ENTER: f64 = 0.75;

/// Where a scripted drive is HEADED: the nearest settlement that is not
/// the one the car is standing in, and its own name.
///
/// It is worked out once and kept, because the nearest town changes the
/// moment you arrive at it and a car that re-picked every frame would
/// turn round in the street it had just reached.
#[derive(Resource, Default)]
pub struct Goal(pub Option<DVec3>, pub String);

/// Pick that goal, once, when a scripted drive begins, and MARK it on
/// the map, which is what a player does before setting off and what a
/// headless run has no pointer to do: the compass strip's tick and the
/// map's first leg are then in the picture.
pub fn aim_drive(
    args: Res<Args>,
    here: crate::world::Surface,
    thefts: Res<Thefts>,
    mut goal: ResMut<Goal>,
    mut markers: ResMut<crate::map::Markers>,
) {
    if args.drive == 0 || goal.0.is_some() {
        return;
    }
    let Some(car) = thefts.driving() else { return };
    let radius = here.world().planet.radius;
    let at = car.car.dir;
    let mut best: Option<(f64, usize)> = None;
    for (k, town) in here.world().towns.iter().enumerate() {
        let gone = town.dir.angle_between(at) * radius;
        // Past the town it is standing IN, whose own streets are the
        // ground under the wheels.
        if gone < town.radius * freeport_core::town::OUTLINE + 200.0 {
            continue;
        }
        if best.is_none_or(|(d, _)| gone < d) {
            best = Some((gone, k));
        }
    }
    let Some((gone, k)) = best else { return };
    *goal = Goal(Some(here.world().towns[k].dir), format!("town {k}"));
    if markers.0.is_empty() {
        markers.0.push(here.world().towns[k].dir);
    }
    info!(
        "driving for town {k}, {:.2} km off over the ground",
        gone / 1000.0
    );
}
