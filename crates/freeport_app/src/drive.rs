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
use freeport_core::walker::{Bounds, Walker};

/// How far behind the car the camera sits and how far over it, metres.
/// A CHASE camera rather than the driver's own eye, because the point of
/// stealing a car is the car, and a first person view of one is a view
/// of the inside of its own bonnet.
const CHASE: f64 = 8.5;
const LIFT: f64 = 3.2;

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
            let field = ground.0.underfoot(dir * car.car.foot, 8.0);
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
            let Some((who, tint, dir, fwd)) = nearest_agent(crowds, ground, here, now) else {
                return;
            };
            let field = ground.0.underfoot(here, 8.0);
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
    now: f64,
) -> Option<((usize, usize), usize, DVec3, DVec3)> {
    crowds
        .cars_near(ground.0.planet.radius, here, driver::REACH, now)
        .into_iter()
        .min_by(|a, b| {
            (a.2 * ground.0.planet.radius)
                .distance(here)
                .total_cmp(&(b.2 * ground.0.planet.radius).distance(here))
        })
}

/// One frame at the wheel: the pedals, the wheel, and where the camera
/// then sits.
pub fn drive_car(
    controls: Controls,
    args: Res<Args>,
    ground: Res<Ground>,
    mut thefts: ResMut<Thefts>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
    mut left: Local<Option<u32>>,
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
    let run = left.get_or_insert(args.drive);
    if *run > 0 {
        input = Drive {
            throttle: 1.0,
            ..Default::default()
        };
        dt = 1.0 / 60.0;
        *run -= 1;
    }
    let car = &mut thefts.cars[k].car;
    let field = ground.0.underfoot(car.dir * car.foot, 12.0);
    // A car floats on nothing: the sea is where a stolen car stops, so
    // the bounds it drives against carry no water to be held up by.
    let bounds = Bounds {
        sea: 0.0,
        ..ground.0.bounds
    };
    car.update(&field, &bounds, &input, dt);
    eye.0 = WorldPos(ground.1 + chase(car));
    status.walker = format!(
        "{:.1} m over the mean radius, {:.0} km/h{}, at the wheel",
        car.foot - ground.0.planet.radius,
        car.speed.abs() * 3.6,
        if car.on_ground { "" } else { ", airborne" },
    );
}

/// Where the camera sits: behind the car and over it, which is what a
/// chase camera is. It never looks at the car from in front, so reverse
/// backs TOWARD the camera rather than swinging it round.
fn chase(car: &Driver) -> DVec3 {
    car.dir * (car.foot + LIFT) - car.fwd * CHASE
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
