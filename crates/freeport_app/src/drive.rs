//! Stealing a car, driving it, and leaving it where you got out.
//!
//! The core's `driver.rs` is the car and this is only what a harness
//! adds: which key is which, where the camera sits, and the two halves of
//! a theft, which are that the stolen car comes OFF the rails and that
//! the one you got out of stays exactly where you left it.
//!
//! **A car you get out of is a car that is still there**, which is
//! tenebris's oldest open bug written down as a rule in `CLAUDE.md`: its
//! ship vanished on exit for years because exit threw the ride away. A
//! theft here keeps ONE `Driver` whether you are in the car or not, so
//! there is nowhere for the pose to be lost: getting out sets a flag and
//! nothing else, and getting back in reads the same pose it wrote.
//!
//! And the agent stays STOLEN for good. Putting it back on the rails
//! would teleport it to wherever the closed form says it should have got
//! to, which is a car jumping across the street the moment you walk away
//! from it.

use crate::traffic::Crowds;
use crate::walk::OnFoot;
use crate::{Args, Controls, Eye, Ground, Status};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::driver::{self, Drive, Driver};
use freeport_core::pos::WorldPos;
use freeport_core::town::{self, lot_frame};
use freeport_core::traffic::Streets;
use freeport_core::walker::{Bounds, Walker};

/// How far behind the car the camera sits and how far over it, metres.
/// A CHASE camera rather than the driver's own eye, because the point of
/// stealing a car is the car, and a first person view of one is a view
/// of the inside of its own bonnet.
const CHASE: f64 = 8.5;
const LIFT: f64 = 3.2;
/// How many sixtieths of a second a SCRIPTED drive steps per rendered
/// frame. Sixty, so one rendered frame is one second of driving.
const SUB_STEPS: usize = 60;
/// How far over the road the camera AIMS, metres: the car's own roof.
const AIM: f64 = 1.4;
/// How many town radii a SCRIPTED theft reaches, which is the whole town.
const SCRIPT_REACH: f64 = 200.0;

/// One car that has been stolen: which agent it was, where it is, and
/// whether anybody is in it.
pub struct Theft {
    /// Which of `Crowds::towns` and which of that town's agents. Off the
    /// rails from here on, so the traffic never draws it again.
    pub who: (usize, usize),
    /// Which tint it was, so the car you stole is the car you drive.
    pub tint: usize,
    /// Where it is, driven or parked. ONE pose whether you are in it or
    /// not, because two would be two things to keep in step and the one
    /// that goes stale is the parked one.
    pub car: Driver,
}

/// Every car the player has taken, and which of them is being driven.
#[derive(Resource, Default)]
pub struct Thefts {
    pub cars: Vec<Theft>,
    pub at_wheel: Option<usize>,
}

impl Thefts {
    /// Which agents are off the rails, sorted, for the traffic to skip.
    pub fn stolen(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = self.cars.iter().map(|t| t.who).collect();
        out.sort_unstable();
        out
    }

    /// The car being driven, if any.
    pub fn driving(&self) -> Option<&Theft> {
        self.at_wheel.map(|k| &self.cars[k])
    }
}

/// A car the player owns, drawn from its own `Theft` every frame.
#[derive(Component)]
pub struct Stolen(pub usize);

/// What a theft looks at: the ground the car stands on, the crowds it is
/// taken from, and the clock that says where the rails have got to. One
/// thing, because a system that reads the world is not a system with
/// eight arguments, which is `traffic::Here`'s own reason.
#[derive(SystemParam)]
pub struct Street<'w> {
    ground: Res<'w, Ground>,
    fabric: Res<'w, crate::world::Fabric>,
    crowds: Option<Res<'w, Crowds>>,
    time: Res<'w, Time>,
    args: Res<'w, Args>,
}

/// E gets in and out. The one key, because getting into the car you are
/// standing beside and getting out of the one you are in are the same
/// verb and a player looks for one button.
pub fn board(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    street: Street,
    walker: Option<Res<OnFoot>>,
    mut thefts: ResMut<Thefts>,
    mut status: ResMut<Status>,
    mut scripted: Local<bool>,
) {
    // A scripted theft, for a headless run with nobody to press E. It
    // waits for the streamer to have put a car in reach rather than
    // firing on frame nought, because the crowds are turned out after
    // the world is and a theft of nothing is a walk.
    let script = street.args.drive > 0 && !*scripted && thefts.cars.is_empty();
    if !keys.just_pressed(KeyCode::KeyE) && !script {
        return;
    }
    let ground = &street.ground;
    match (thefts.at_wheel, walker) {
        // Out of the car, onto the road beside it. The car keeps its
        // pose, because the pose is the car's and was never the ride's.
        (Some(_), _) => {
            let Some(car) = thefts.driving() else { return };
            let dir = car.car.kerbside(ground.0.planet.radius);
            let heading = car.car.fwd;
            let field = street
                .fabric
                .underfoot(&ground.0.planet, dir * car.car.foot, 8.0);
            let walker = Walker::enter(&field, &ground.0.bounds, dir, heading);
            commands.insert_resource(OnFoot(walker));
            thefts.at_wheel = None;
            status.walker = "out of the car".to_string();
        }
        // Into the nearest car within reach: one already stolen and
        // parked, else one still on the rails, which this takes.
        (None, Some(w)) => {
            let Some(crowds) = &street.crowds else { return };
            let here = w.0.dir * w.0.foot;
            if let Some(k) = nearest_parked(&thefts, here) {
                thefts.at_wheel = Some(k);
                commands.remove_resource::<OnFoot>();
                status.walker = "back in the car".to_string();
                return;
            }
            let now = street.time.elapsed_secs_f64();
            // A SCRIPTED theft reaches as far as the town, and a player
            // reaches as far as his arm. The flag exists to photograph a
            // car and it has no legs to walk over with: at a player's
            // own reach a headless run stands on the kerb waiting for
            // one to brush past, which on the first try took seven
            // minutes of rendering to happen by luck.
            let reach = if street.args.drive > 0 && thefts.cars.is_empty() {
                town::OUTLINE * SCRIPT_REACH
            } else {
                driver::REACH
            };
            let built = street.fabric.standing();
            let Some((who, tint, at, fwd)) =
                nearest_agent(crowds, ground, here, reach, now, &built)
            else {
                return;
            };
            let dir = at.normalize();
            let field = street.fabric.underfoot(&ground.0.planet, here, 8.0);
            let car = Driver::board(&field, &ground.0.bounds, dir, fwd);
            thefts.cars.push(Theft { who, tint, car });
            thefts.at_wheel = Some(thefts.cars.len() - 1);
            commands.remove_resource::<OnFoot>();
            *scripted = true;
            info!("stole a car: town {} agent {}", who.0, who.1);
            status.walker = "at the wheel".to_string();
        }
        (None, None) => {}
    }
}

/// The nearest car the player already owns, if one is within reach.
fn nearest_parked(thefts: &Thefts, here: DVec3) -> Option<usize> {
    thefts
        .cars
        .iter()
        .enumerate()
        .map(|(k, t)| (k, (t.car.dir * t.car.foot).distance(here)))
        .filter(|&(_, d)| d < driver::REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

/// The nearest car still ON THE RAILS, as the agent it is and the place
/// and heading the clock says it has reached. Taking it at exactly where
/// it was is what makes a theft seamless: the car does not jump.
fn nearest_agent(
    crowds: &Crowds,
    ground: &Ground,
    here: DVec3,
    reach: f64,
    now: f64,
    built: &[usize],
) -> Option<((usize, usize), usize, DVec3, DVec3)> {
    crowds
        .cars_near(ground.0.planet.radius, here, reach, now, built)
        .into_iter()
        .min_by(|a, b| a.2.distance(here).total_cmp(&b.2.distance(here)))
}

/// One frame at the wheel: the pedals, the wheel, and where the camera
/// then sits.
pub fn drive_car(
    controls: Controls,
    script: Script,
    here: crate::world::Surface,
    mut thefts: ResMut<Thefts>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
    mut auto: Local<Auto>,
) {
    let Some(k) = thefts.at_wheel else { return };
    let keys = &controls.keys;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f64;
    let mut input = Drive {
        throttle: axis(KeyCode::KeyS, KeyCode::KeyW),
        // A is LEFT, and left is a turn toward the car's own port side,
        // which is the negative of the right hand axis the heading turns
        // about. Reading it the other way steered into the kerb.
        steer: axis(KeyCode::KeyD, KeyCode::KeyA),
        brake: keys.pressed(KeyCode::Space),
    };
    let mut dt = controls.time.delta_secs_f64();
    // A scripted drive holds the throttle and steps a fixed sixtieth,
    // which is `--walk`'s own rule and for the same reason: the frame's
    // own delta on a software rasteriser is most of a second and a car
    // that moves nine metres a frame measures nothing.
    let run = auto.left.get_or_insert(script.args.drive);
    let car = &mut thefts.cars[k].car;
    let mut steps = 1;
    if *run > 0 {
        // A scripted drive goes SOMEWHERE: the nearest settlement that
        // is not the one it is standing in. Driving straight ahead
        // measures the car and says nothing about whether the world has
        // anywhere to drive TO, which is what the owner asked for.
        input = Drive {
            throttle: 1.0,
            ..Default::default()
        };
        dt = 1.0 / 60.0;
        // A whole SECOND of driving a rendered frame, in sixtieths. A
        // frame of this world on a software rasteriser is most of a
        // second, so a scripted drive stepped one sixtieth a frame
        // covers 480 m in half an hour of rendering, and the nearest
        // settlement is nine kilometres off: the flag could photograph
        // a car and never a JOURNEY. The step stays a sixtieth, which
        // is what keeps the drive the same drive on any machine.
        steps = SUB_STEPS;
        *run -= 1;
    }
    let was = car.dir;
    let field = here.underfoot(car.dir * car.foot, 12.0);
    // A car floats on nothing: the sea is where a stolen car stops, so
    // the bounds it drives against carry no water to be held up by.
    let bounds = Bounds {
        sea: 0.0,
        ..here.world().bounds
    };
    for _ in 0..steps {
        // The wheel is re-read every sub step, or a scripted drive
        // holds one bearing for a whole second and weaves round its own
        // line at sixteen metres a second.
        let input = if steps > 1 {
            // The AIM and not the goal, and that is the whole of what a
            // road bought a scripted drive. `steer_for` computed the
            // point on the tarmac and handed it to the wheel of the
            // OUTER input, which a sub stepped drive then threw away and
            // replaced with `car.toward(goal)`: the road following was
            // written, tested and never once driven on, and the car went
            // on wedging itself against the same building it always had.
            let aim = auto.aim(&script, &here, car);
            auto.drive(car, aim, dt, here.world().planet.radius)
        } else {
            input
        };
        car.update(&field, &bounds, &input, dt);
    }
    if steps > 1 {
        auto.say(car, was, script.goal.0, here.world().planet.radius);
    }
    eye.0 = WorldPos(here.centre() + chase(car));
    status.walker = format!(
        "{:.1} m over the mean radius, {:.0} km/h{}, at the wheel{}",
        car.foot - here.world().planet.radius,
        car.speed.abs() * 3.6,
        if car.on_ground { "" } else { ", airborne" },
        script.goal.0.map_or(String::new(), |g| format!(
            " | {} {:.2} km off",
            script.goal.1,
            car.dir.angle_between(g) * here.world().planet.radius / 1000.0
        )),
    );
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
}

impl Auto {
    /// How far a scripted drive has come, every half minute of it.
    ///
    /// A drive between two towns is a JOURNEY or it is not, and the one
    /// number that says which is the GROUND made. The speedometer is not
    /// that number: the car that wedged itself out of the port read
    /// 4 km/h while making nought metres for two minutes.
    fn say(&mut self, car: &Driver, was: DVec3, goal: Option<DVec3>, radius: f64) {
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
        info!("  steering for {} {:.0} m off", self.why.0, self.why.1);
    }

    /// Where a scripted drive STEERS FOR: the road when it is on one,
    /// the way out of the town along that town's own streets when it is
    /// in one, and the town it is driving to when it is neither.
    ///
    /// Three answers and not two, because a car stolen in the middle of
    /// the port is 700 m from the nearest tarmac, which is further than
    /// `OFF_ROAD`: `Network::follow` gives up there and the drive aimed
    /// at a settlement nine kilometres off, which from a street between
    /// two buildings is aiming at a wall. The streets are the way out and
    /// `Streets::route` walks them.
    fn aim(
        &mut self,
        script: &Script,
        here: &crate::world::Surface,
        car: &Driver,
    ) -> Option<DVec3> {
        let world = here.world();
        let (goal, radius) = (script.goal.0?, world.planet.radius);
        // Off the road: get TO it, and along the paving while there is
        // paving to follow.
        let (what, at) = match script.roads.follow(world, car.dir, goal, AHEAD, radius) {
            Some(p) => ("the road", p),
            None => match script.roads.mouth(world, car.dir) {
                None => ("its town", goal),
                Some(mouth) => match self.through_town(world, car.dir * car.foot, mouth) {
                    Some(p) => ("the streets", p),
                    None => ("the tarmac", mouth),
                },
            },
        };
        let aim = at.normalize();
        self.why = (what, car.dir.angle_between(aim) * radius);
        Some(aim)
    }

    /// The next crossing but one along the streets of whatever town the
    /// car is standing in, toward the tarmac. Nothing when it is in no
    /// town, or when the paving never joined the two.
    fn through_town(
        &mut self,
        world: &crate::world::World,
        at: DVec3,
        mouth: DVec3,
    ) -> Option<DVec3> {
        let radius = world.planet.radius;
        let dir = at.normalize_or(DVec3::Y);
        let (k, town) = world.towns.iter().enumerate().find(|(_, t)| {
            t.dir.angle_between(dir) * radius < t.radius * freeport_core::town::OUTLINE
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
    fn drive(&mut self, car: &Driver, goal: Option<DVec3>, dt: f64, radius: f64) -> Drive {
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
        Drive {
            throttle: 1.0,
            steer: want,
            brake: false,
        }
    }
}

/// What a SCRIPTED drive is: the flags it was given and where it is
/// headed. One thing, because they are one question and asking it as
/// two took `drive_car` over Bevy's own parameter limit.
#[derive(SystemParam)]
pub struct Script<'w> {
    args: Res<'w, Args>,
    goal: Res<'w, Goal>,
    /// The tarmac, so a scripted drive FOLLOWS the road to its town
    /// rather than aiming through whatever stands between.
    roads: Res<'w, crate::roads::Network>,
}

/// How far down the road a scripted drive looks, metres: far enough that
/// the wheel is not sawed at and near enough that a bend is taken rather
/// than cut. Pure pursuit's own one knob.
const AHEAD: f64 = 400.0;

/// Where a scripted drive is HEADED: the nearest settlement that is not
/// the one the car is standing in, and its own name.
///
/// It is worked out once and kept, because the nearest town changes the
/// moment you arrive at it and a car that re-picked every frame would
/// turn round in the street it had just reached.
#[derive(Resource, Default)]
pub struct Goal(pub Option<DVec3>, pub String);

/// Pick that goal, once, when a scripted drive begins.
pub fn aim_drive(
    args: Res<Args>,
    here: crate::world::Surface,
    thefts: Res<Thefts>,
    mut goal: ResMut<Goal>,
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
    info!(
        "driving for town {k}, {:.2} km off over the ground",
        gone / 1000.0
    );
}

/// Where the camera sits: behind the car and over it, which is what a
/// chase camera is. It never looks at the car from in front, so reverse
/// backs TOWARD the camera rather than swinging it round.
fn chase(car: &Driver) -> DVec3 {
    car.dir * (car.foot + LIFT) - car.fwd * CHASE
}

/// Which way that camera looks: at the car's own ROOF rather than along
/// its heading, so the car sits in the middle of the frame instead of
/// under the bottom of it.
pub fn look_at(car: &Driver) -> DVec3 {
    (car.dir * (car.foot + AIM) - chase(car)).normalize()
}

/// Draw every stolen car where its own `Theft` says it is, driven or
/// parked, and spawn one the first time it is taken.
pub fn show_cars(
    mut commands: Commands,
    thefts: Res<Thefts>,
    crowds: Option<Res<Crowds>>,
    planets: Res<crate::planets::Planets>,
    frame: Res<crate::stream::Frame>,
    ground: Res<Ground>,
    mut cars: Query<(Entity, &Stolen, &mut Transform)>,
) {
    let Some(crowds) = crowds else { return };
    // A stolen car belongs to ONE body, which is the crowd's own rule:
    // off it, the car stays parked where it was rather than being drawn
    // against another planet's radius and centre, and it is there again
    // when you come back. That is the same rule as getting out of it.
    if !crowds.on_body(planets.active) {
        for (e, _, _) in &cars {
            commands.entity(e).despawn();
        }
        return;
    }
    let mut drawn = vec![false; thefts.cars.len()];
    for (_, stolen, mut tf) in &mut cars {
        let Some(theft) = thefts.cars.get(stolen.0) else {
            continue;
        };
        drawn[stolen.0] = true;
        let (at, turn) = place(&theft.car);
        tf.translation = frame.0.local(WorldPos(ground.1 + at));
        tf.rotation = turn;
    }
    for (k, done) in drawn.iter().enumerate() {
        if *done {
            continue;
        }
        let theft = &thefts.cars[k];
        let (at, turn) = place(&theft.car);
        crowds.spawn_car(
            &mut commands,
            theft.tint,
            Transform {
                translation: frame.0.local(WorldPos(ground.1 + at)),
                rotation: turn,
                scale: Vec3::ONE,
            },
            Stolen(k),
        );
    }
}

/// Where a car stands and which way it points, in the planet's frame.
/// The figure's own frame is x to its right, y the way it is going and
/// z up, which is the same right handed basis the traffic places its
/// cars in, so the mesh is the same mesh and is not wound inside out.
fn place(car: &Driver) -> (DVec3, Quat) {
    let up = car.dir;
    let basis = Mat3::from_cols(car.right().as_vec3(), car.fwd.as_vec3(), up.as_vec3());
    (up * car.foot, Quat::from_mat3(&basis))
}
