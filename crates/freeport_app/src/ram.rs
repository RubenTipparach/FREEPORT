//! RAMMING, in the harness: the car at the wheel against every other
//! car, and the cars it has knocked off the rails rolling to a stop.
//!
//! What two cars exchange is `ram` in the core; what this decides is
//! WHICH cars are near enough to ask, and what a car that was on the
//! rails becomes once it has been hit, which is a `Theft` with nobody at
//! the wheel: drawn by `show_cars`, a box to the player's car, skipped
//! by the traffic for good, and boarded with E like any other car the
//! player left standing. It is the theft's own rule, because a car that
//! has been hit has left the closed form exactly as one that has been
//! taken, and putting it back would be a car jumping across the street
//! to wherever the clock says it should have got to.
//!
//! The rails cars' own velocity is read off the rails a tick apart,
//! because a rail is a place and not a velocity, and it is what makes
//! the OTHER half of the owner's ask true: a car on the rails that
//! drives into the player's shoves the player, and comes off the rails
//! for it.

use crate::drive::{Rails, Theft, Thefts, CAR_REACH};
use crate::traffic::Crowds;
use crate::world::Surface;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::driver::{Drive, Driver};
use freeport_core::fuel::Tank;
use freeport_core::noise::hash3;
use freeport_core::ram::{self, Body};
use freeport_core::walker::Bounds;

/// How far apart the rails are read to get a car's velocity, seconds: a
/// sixtieth, which is the step the drive itself is taken in.
const TICK: f64 = 1.0 / 60.0;

/// Every car on the rails within `CAR_REACH` of a point: which it is,
/// its tint, where it stands, which way it points and how fast it is
/// going.
fn rails_near(
    crowds: &Crowds,
    here: &Surface,
    at: DVec3,
    now: f64,
) -> Vec<(Rails, usize, DVec3, DVec3, DVec3)> {
    let world = here.world();
    let radius = world.planet.radius;
    let built = here.fabric.standing();
    let mut out = Vec::new();
    // A little wider a tick ago, so a car arriving at the edge of the
    // reach is found at both times and gets its velocity.
    let then = crowds.cars_near(radius, at, CAR_REACH + 5.0, now - TICK, &built);
    for (who, tint, place, fwd) in crowds.cars_near(radius, at, CAR_REACH, now, &built) {
        let vel = then
            .iter()
            .find(|c| c.0 == who)
            .map_or(DVec3::ZERO, |c| (place - c.2) / TICK);
        out.push((Rails::Town(who.0, who.1), tint, place, fwd, vel));
    }
    let then = crowds.road_cars_near(world, at, CAR_REACH + 5.0, now - TICK);
    for (who, tint, place, fwd) in crowds.road_cars_near(world, at, CAR_REACH, now) {
        let vel = then
            .iter()
            .find(|c| c.0 == who)
            .map_or(DVec3::ZERO, |c| (place - c.2) / TICK);
        out.push((Rails::Road(who.0, who.1), tint, place, fwd, vel));
    }
    out
}

/// A driven car as a collision sees it.
fn body(car: &Driver) -> Body {
    Body {
        at: car.dir * car.foot,
        fwd: car.fwd,
        vel: car.velocity(),
    }
}

/// A car on the rails taken OFF them by a hit: standing where the rails
/// had it, going the way they had it going, with whatever was in its
/// tank, and then knocked.
fn knock_off(
    here: &Surface,
    bounds: &Bounds,
    (who, tint, at, fwd, vel): (Rails, usize, DVec3, DVec3, DVec3),
    hit: ram::Hit,
    now: f64,
) -> Theft {
    let field = here.underfoot(at, 8.0);
    let mut car = Driver::board(&field, bounds, at.normalize(), fwd);
    car.speed = vel.dot(car.fwd);
    let (a, b) = who.index();
    car.tank = Tank::part(hash3(a as i64, b as i64, 0x7A, 0x9A5));
    car.knock(hit.normal * hit.dv, hit.spin);
    info!(
        "rammed {who:?}: it takes {:.1} m/s and turns at {:.2} rad/s",
        hit.dv, hit.spin
    );
    Theft {
        who,
        tint,
        car,
        swing: fwd,
        hit: now,
    }
}

/// The car at the wheel against every car near it, and every car nobody
/// is in rolling on with nothing on the pedals. Before `drive_car`, so
/// the knock is taken off the speed the car actually arrived at and not
/// off what the wall it then meets leaves of it.
pub fn ram_cars(
    here: Surface,
    crowds: Option<Res<Crowds>>,
    mut thefts: ResMut<Thefts>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs_f64().min(0.05);
    let bounds = Bounds {
        sea: 0.0,
        ..here.world().bounds
    };
    if let Some(k) = thefts.at_wheel {
        let a = body(&thefts.cars[k].car);
        // What the player's own car takes back: every knock it hands
        // out, the other way.
        let mut took = DVec3::ZERO;
        let mut knocked = Vec::new();
        if let Some(crowds) = crowds.as_deref().filter(|c| c.on_body(here.body())) {
            for car in rails_near(crowds, &here, a.at, now) {
                // Off the rails already: it is a theft and is met below.
                if thefts.cars.iter().any(|t| t.who == car.0) {
                    continue;
                }
                let b = Body {
                    at: car.2,
                    fwd: car.3,
                    vel: car.4,
                };
                if let Some(hit) = ram::impact(&a, &b) {
                    took -= hit.normal * hit.dv;
                    knocked.push(knock_off(&here, &bounds, car, hit, now));
                }
            }
        }
        for j in 0..thefts.cars.len() {
            if j == k {
                continue;
            }
            let Some(hit) = ram::impact(&a, &body(&thefts.cars[j].car)) else {
                continue;
            };
            let other = &mut thefts.cars[j];
            other.car.knock(hit.normal * hit.dv, hit.spin);
            other.hit = now;
            took -= hit.normal * hit.dv;
        }
        if took != DVec3::ZERO {
            let mine = &mut thefts.cars[k];
            mine.car.knock(took, 0.0);
            mine.hit = now;
        }
        thefts.cars.extend(knocked);
    }
    let wheel = thefts.at_wheel;
    for (j, theft) in thefts.cars.iter_mut().enumerate() {
        let car = &theft.car;
        if Some(j) == wheel || (car.speed == 0.0 && car.shove == DVec3::ZERO && car.spin == 0.0) {
            continue;
        }
        let field = here.underfoot(car.dir * car.foot, 12.0);
        theft.car.update(&field, &bounds, &Drive::default(), dt);
    }
}
