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
use freeport_core::driver::{self, Driver};
use freeport_core::field::hash3;
use freeport_core::fuel::Tank;
use freeport_core::pos::WorldPos;
use freeport_core::town;
use freeport_core::traffic::Streets;
use freeport_core::walker::{Bounds, Walker};

mod script;
pub use script::*;

/// How far behind the car the camera sits and how far over it, metres.
/// A CHASE camera rather than the driver's own eye, because the point of
/// stealing a car is the car, and a first person view of one is a view
/// of the inside of its own bonnet.
const CHASE: f64 = 8.5;
const LIFT: f64 = 3.2;
/// How fast that camera comes round BEHIND the car, seconds.
///
/// A time constant and not a share of a frame, so the swing takes the
/// same wall time at twenty frames a second as at a hundred and twenty,
/// which is swarm-demo's own rule for its sliding deck. Placed rigidly
/// off `car.fwd` the camera is welded to the roof: the car never turns
/// on screen, the WORLD whips round it, and a corner reads as the
/// horizon snapping rather than as the car going round.
const SWING: f64 = 0.40;
/// How much further back and higher it sits at the car's top speed,
/// metres. A camera at one distance says nothing about how fast the car
/// is going; one that pulls back and lifts as the speed rises does, and
/// it is what makes the same corner read as fast.
const STRETCH: f64 = 4.0;
const RISE: f64 = 1.2;
/// How many sixtieths of a second a SCRIPTED drive steps per rendered
/// frame. Sixty, so one rendered frame is one second of driving.
const SUB_STEPS: usize = 60;
/// How far over the road the camera AIMS, metres: the car's own roof.
const AIM: f64 = 1.4;
/// How many town radii a SCRIPTED theft reaches, which is the whole town.
const SCRIPT_REACH: f64 = 200.0;

/// Which car on the RAILS a car off them was: a town's agent, or a
/// road's commuter. It is what the traffic skips for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rails {
    /// Which of `Crowds::towns` and which of that town's agents.
    Town(usize, usize),
    /// Which road of the world and which of its commuters.
    Road(usize, usize),
}

impl Rails {
    /// The two numbers that name it, whichever kind it is.
    pub fn index(self) -> (usize, usize) {
        match self {
            Rails::Town(a, b) | Rails::Road(a, b) => (a, b),
        }
    }
}

/// How long after a hit two cars pass through each other's boxes,
/// seconds, so the car that was shoved away is not also a wall to the
/// one that shoved it: the knock has already taken the speed off, and a
/// wall on top of it took `CRASH` of what was left, which is a rammer
/// that stops dead against the car it just sent on its way.
pub const CLEAR: f64 = 0.25;

/// One car that is OFF the rails: taken by the player or knocked off
/// them (`ram`), where it is, and whether anybody is in it.
pub struct Theft {
    /// Which car it was on the rails. Off them from here on, so the
    /// traffic never draws it again.
    pub who: Rails,
    /// Which tint it was, so the car you stole is the car you drive.
    pub tint: usize,
    /// Where it is, driven or parked. ONE pose whether you are in it or
    /// not, because two would be two things to keep in step and the one
    /// that goes stale is the parked one.
    pub car: Driver,
    /// The heading the CHASE CAMERA sits behind: the car's own, eased
    /// toward it, so the view swings round a corner rather than cutting
    /// to it. Per THEFT rather than one for the player, so getting back
    /// into a parked car resumes behind it instead of whipping round
    /// from wherever the last one was pointing.
    pub swing: DVec3,
    /// When it was last hit, or hit something, seconds on the clock:
    /// for `CLEAR`.
    pub hit: f64,
}

impl Theft {
    /// Where that heading has got to this frame.
    pub fn swung(&mut self, dt: f64) -> DVec3 {
        // Squared to the car's own up FIRST, or a heading carried over a
        // curving planet leaves the tangent plane and the camera sinks
        // into the ground a hundred kilometres down the road.
        let car = &self.car;
        let held = (self.swing - car.dir * self.swing.dot(car.dir)).normalize_or(car.fwd);
        self.swing = slerp(held, car.fwd, 1.0 - (-dt / SWING).exp());
        self.swing
    }
}

/// The shorter way round from one unit vector to another, `t` of the way
/// there.
///
/// SLERP and not a lerp of the two: a lerp crosses the chord, so it
/// turns fastest in the middle of a swing and the camera arrives with a
/// jerk. It is the owner's own word for what a chase camera should do.
fn slerp(from: DVec3, to: DVec3, t: f64) -> DVec3 {
    let angle = from.dot(to).clamp(-1.0, 1.0).acos();
    let s = angle.sin();
    // Dead ahead or dead behind, where there is no plane to turn in and
    // never a NaN out of a divide by nought.
    if s <= 1e-9 || !s.is_finite() {
        return to;
    }
    (from * ((angle * (1.0 - t)).sin() / s) + to * ((angle * t).sin() / s)).normalize_or(to)
}

/// Every car the player has taken, and which of them is being driven.
#[derive(Resource, Default)]
pub struct Thefts {
    pub cars: Vec<Theft>,
    pub at_wheel: Option<usize>,
}

impl Thefts {
    /// Which of the towns' agents are off the rails, sorted, for the
    /// traffic to skip.
    pub fn stolen(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = self
            .cars
            .iter()
            .filter_map(|t| match t.who {
                Rails::Town(a, b) => Some((a, b)),
                Rails::Road(..) => None,
            })
            .collect();
        out.sort_unstable();
        out
    }

    /// And which of the roads' commuters, for the highway to skip.
    pub fn knocked_roads(&self) -> Vec<(usize, usize)> {
        self.cars
            .iter()
            .filter_map(|t| match t.who {
                Rails::Road(a, b) => Some((a, b)),
                Rails::Town(..) => None,
            })
            .collect()
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
            let mut car = Driver::board(&field, &ground.0.bounds, dir, fwd);
            // With whatever was in its tank: nobody parks full.
            car.tank = Tank::part(hash3(who.0 as i64, who.1 as i64, 0x7A, 0x9A5));
            let swing = car.fwd;
            thefts.cars.push(Theft {
                who: Rails::Town(who.0, who.1),
                tint,
                car,
                swing,
                hit: f64::NEG_INFINITY,
            });
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
pub(crate) fn nearest_parked(thefts: &Thefts, here: DVec3) -> Option<usize> {
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
    crowds: Option<Res<Crowds>>,
    mut thefts: ResMut<Thefts>,
    mut dash: Dash,
    mut auto: Local<Auto>,
) {
    let Some(k) = thefts.at_wheel else { return };
    let (input, dt, steps) = pedals(&controls, &script, &mut auto);
    // The OTHER CARS, as boxes, so the one with somebody at the wheel is
    // stopped by them. They are gathered BEFORE the field is borrowed
    // and live as long as it does, which is what lets a `Built` carry
    // the town's own walls and something that moves in one list.
    let others = around(&crowds, &here, &thefts, k, controls.time.elapsed_secs_f64());
    let standing = thefts.cars[k].car.dir;
    let foot = thefts.cars[k].car.foot;
    let mut field = here.underfoot(standing * foot, 12.0);
    field.blocks.extend(others.iter());
    let car = &mut thefts.cars[k].car;
    let was = car.dir;
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
    dash.status.walker = format!(
        "{:.1} m over the mean radius, {:.0} km/h{}, at the wheel{} | {}",
        car.foot - here.world().planet.radius,
        car.speed.abs() * 3.6,
        if car.on_ground { "" } else { ", airborne" },
        script.goal.0.map_or(String::new(), |g| format!(
            " | {} {:.2} km off",
            script.goal.1,
            car.dir.angle_between(g) * here.world().planet.radius / 1000.0
        )),
        crate::fuel::gauge(car, &dash.wallet, &dash.roads),
    );
    // The camera eases over the DRIVING this frame carried, which on a
    // scripted run is `steps` sixtieths and not the frame's own delta: a
    // rendered frame there is a whole second of driving, and a camera
    // eased by the frame would still be pointing where the car was a
    // second ago in every picture the flag takes.
    let theft = &mut thefts.cars[k];
    let swing = theft.swung(dt * steps as f64);
    dash.eye.0 = WorldPos(here.centre() + chase(&theft.car, swing));
}

/// How far round a car the other cars are gathered as colliders,
/// metres. Its own outline reaches 2.05 m and another car is 4.1 m long,
/// so anything past this cannot be met inside one frame at any speed a
/// car goes; and the list is the few a built town has out anyway.
pub(crate) const CAR_REACH: f64 = 30.0;

/// Every OTHER car near the one being driven, as the box each is: the
/// traffic still on the rails, and any the player has parked.
///
/// The rails cars are asked the same `Crowds::cars_near` a theft asks,
/// so a car that is collided with is at exactly where it is drawn: a
/// second answer about where a car is would be a body stopped by
/// nothing a player can see.
///
/// What is MISSING and is named rather than hidden: a car on the rails
/// does not know this car is there. It is a closed form function of its
/// town and the clock, and knowing would mean state, which is the one
/// thing rails do not have. So the player is stopped by the traffic and
/// the traffic drives on through the player.
fn around(
    crowds: &Option<Res<Crowds>>,
    here: &crate::world::Surface,
    thefts: &Thefts,
    driving: usize,
    now: f64,
) -> Vec<freeport_core::field::Block> {
    let radius = here.world().planet.radius;
    let at = thefts.cars[driving].car.dir * thefts.cars[driving].car.foot;
    let mut out = Vec::new();
    if let Some(crowds) = crowds {
        if crowds.on_body(here.body()) {
            let built = here.fabric.standing();
            let town = crowds.cars_near(radius, at, CAR_REACH, now, &built);
            // And the ones out on the ROADS, which is where a car at
            // 160 km/h actually meets another one.
            let road = crowds.road_cars_near(here.world(), at, CAR_REACH, now);
            // Not the GHOST of a car that is off the rails: the rails
            // still say where a stolen or a knocked car would have got
            // to, and that was an invisible wall driving down the road.
            let (stolen, knocked) = (thefts.stolen(), thefts.knocked_roads());
            let town = town
                .into_iter()
                .filter(|c| stolen.binary_search(&c.0).is_err());
            let road = road.into_iter().filter(|c| !knocked.contains(&c.0));
            for (_, _, place, fwd) in town.chain(road) {
                out.push(driver::car_box(place, fwd, place.normalize_or(DVec3::Y)));
            }
        }
    }
    // And the ones the player has already taken and left standing: a car
    // you got out of is a car that is still there, so it is still
    // something to drive into. Not one hit inside `CLEAR`, which is on
    // its way.
    for (i, theft) in thefts.cars.iter().enumerate() {
        if i == driving || now - theft.hit < CLEAR {
            continue;
        }
        let car = &theft.car;
        let place = car.dir * car.foot;
        if place.distance(at) < CAR_REACH {
            out.push(driver::car_box(place, car.fwd, car.dir));
        }
    }
    out
}

/// What a frame at the wheel REPORTS: where the eye ends up, and the
/// line of text that says what the car is doing.
///
/// One thing, because they are the two things this system WRITES and a
/// system with eight arguments is a system missing a struct, which is
/// this project's own rule.
#[derive(SystemParam)]
pub struct Dash<'w> {
    pub eye: ResMut<'w, Eye>,
    pub status: ResMut<'w, Status>,
    /// What the gauge reads: the purse and where the next pump is.
    pub wallet: Res<'w, crate::fuel::Wallet>,
    pub roads: Res<'w, crate::roads::Network>,
}

/// Where the camera sits: behind the car and over it, which is what a
/// chase camera is. It never looks at the car from in front, so reverse
/// backs TOWARD the camera rather than swinging it round.
fn chase(car: &Driver, fwd: DVec3) -> DVec3 {
    // How hard it is being driven, nought to one, which is what decides
    // how far back the camera stands.
    let hurry = (car.speed.abs() / freeport_core::driver::TOP).clamp(0.0, 1.0);
    car.dir * (car.foot + LIFT + RISE * hurry) - fwd * (CHASE + STRETCH * hurry)
}

/// Which way that camera looks: at the car's own ROOF rather than along
/// its heading, so the car sits in the middle of the frame instead of
/// under the bottom of it.
pub fn look_at(car: &Driver, fwd: DVec3) -> DVec3 {
    (car.dir * (car.foot + AIM) - chase(car, fwd)).normalize()
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
    for (e, stolen, mut tf) in &mut cars {
        let Some(theft) = thefts.cars.get(stolen.0) else {
            continue;
        };
        drawn[stolen.0] = true;
        let (at, turn) = place(&theft.car);
        tf.translation = frame.0.local(WorldPos(ground.1 + at));
        tf.rotation = turn;
        commands
            .entity(e)
            .insert(crate::traffic::Travelled(theft.car.gone));
    }
    for (k, done) in drawn.iter().enumerate() {
        if *done {
            continue;
        }
        let theft = &thefts.cars[k];
        let (at, turn) = place(&theft.car);
        let car = crowds.spawn_car(
            &mut commands,
            theft.tint,
            Transform {
                translation: frame.0.local(WorldPos(ground.1 + at)),
                rotation: turn,
                scale: Vec3::ONE,
            },
            Stolen(k),
        );
        // The HEAD LAMPS, on the PLAYER's car and nowhere else, which is
        // `light_lamps`'s own rule: a light near the eye and a number
        // everywhere else. Aimed down the car's own forward, which in a
        // figure's frame is +y while Bevy's spot shines along -z, so the
        // child is turned a quarter onto it.
        for side in [-1.0f32, 1.0] {
            commands.entity(car).with_child((
                SpotLight {
                    intensity: 0.0,
                    range: crate::lamps::HEAD_REACH,
                    inner_angle: 0.16,
                    outer_angle: 0.52,
                    color: crate::lamps::TORCH_COLOUR,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(Vec3::new(side * 0.52, 2.0, 0.68))
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                crate::lamps::Headlamp,
            ));
        }
    }
}

/// Where a car stands and which way it points, in the planet's frame.
/// The figure's own frame is x to its right, y the way it is going and
/// z up, which is the same right handed basis the traffic places its
/// cars in, so the mesh is the same mesh and is not wound inside out.
fn place(car: &Driver) -> (DVec3, Quat) {
    // The GROUND's own up and not the planet's. `car.dir` is up for the
    // planet, so a car drawn off it sits dead level while the hill falls
    // away under it, which is what the owner read off a picture of one
    // parked on a slope. `Driver::lean` is the plane through the four
    // wheels, eased, and it is the core's answer rather than one the app
    // works out for itself.
    //
    // The basis is re-squared to it, because `fwd` is in the PLANET's
    // tangent plane by construction (that is what keeps a car's steering
    // a two dimensional problem) and a basis built from one plane's
    // forward and another's up is not orthogonal: the mesh would shear.
    let up = car.lean;
    let fwd = (car.fwd - up * car.fwd.dot(up)).normalize_or(car.fwd);
    let right = fwd.cross(up).normalize_or(car.right());
    let basis = Mat3::from_cols(right.as_vec3(), fwd.as_vec3(), up.as_vec3());
    (car.dir * car.foot, Quat::from_mat3(&basis))
}
