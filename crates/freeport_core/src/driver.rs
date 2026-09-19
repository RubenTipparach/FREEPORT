//! Driving a car, which is the one townsman that is NOT on rails.
//!
//! Every other car in a town is a closed form function of the town, its
//! own index and the world clock (`traffic.rs`): nothing integrates and
//! nothing is saved. The moment a player gets into one that stops being
//! true of it, and that is exactly this project's own rule rather than an
//! exception to it: **what INTEGRATES is the player, and only the
//! player.** A car with somebody at the wheel IS the player, so it leaves
//! the rails and comes here, and a car nobody is in goes back to being a
//! number.
//!
//! A car is glued to a surface, so its orientation is a BASIS and not a
//! quaternion, which is this file's rule for the walker and for anything
//! else that cannot roll: up really is radial and roll does not exist.
//! The quaternion is for free rotating bodies, which a car on a planet is
//! not.
//!
//! What makes this a car rather than a fast walker is one thing: **it
//! steers with its wheels.** A walker turns on the spot; a car turns by
//! rolling, so its yaw rate is its speed over its turning radius and a
//! car standing still cannot turn at all however hard the wheel is held.
//! That is the bicycle model and it is three lines.

use crate::field::Density;
use crate::figure::{CAR_LONG, CAR_WIDE};
use crate::walker::{self, Bounds};
use glam::{DVec2, DVec3};

/// How fast a town car goes, metres a second: about 58 km/h forward and a
/// crawl backwards, because nobody reverses fast.
pub const TOP: f64 = 16.0;
pub const REVERSE: f64 = 5.0;
/// How hard it pulls, brakes and coasts down, metres a second a second.
const ACCEL: f64 = 5.5;
const BRAKE: f64 = 12.0;
const DRAG: f64 = 2.2;
/// Between the axles, metres. The model's wheels stand at 1.32 either
/// side of its middle, so this is READ off the car that is drawn rather
/// than chosen beside it.
const WHEELBASE: f64 = 2.64;
/// The steering lock, radians. At a wheelbase of 2.64 m a lock of 0.52
/// is a turning radius of `WHEELBASE / tan(LOCK)` = 4.8 m, which is a
/// small car's ten metre turning circle.
const LOCK: f64 = 0.52;
/// How fast the lock is given up as the speed rises, metres a second.
/// Full lock at a crawl and a third of it at the top, because a car that
/// could hold full lock at 58 km/h would be cornering at four gravities.
const TAPER: f64 = 9.0;
/// The tallest kerb the wheels climb, metres. A pavement's is 12 cm, so a
/// stolen car mounts one; a wall is not a kerb and stops it.
const CLIMB: f64 = 0.3;
/// What is left of the speed when the car hits something square on.
const CRASH: f64 = 0.15;
const GRAVITY: f64 = 9.81;
/// The driver's eye over the road, metres, and where a body gets out.
pub const SEAT: f64 = 1.15;
pub const DOOR: f64 = 1.5;
/// How near a car a body has to be to open its door, metres, measured
/// from the car's own middle.
pub const REACH: f64 = 4.0;

/// What the driver asked for this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drive {
    /// Forward and back, each -1, 0 or 1 (or anything between).
    pub throttle: f64,
    /// Which way the wheels are turned, -1 to 1, left positive.
    pub steer: f64,
    pub brake: bool,
}

/// A car with somebody in it: where it stands, which way it points, and
/// how fast it is going.
#[derive(Clone, Debug)]
pub struct Driver {
    /// The direction of the wheels from the planet's centre.
    pub dir: DVec3,
    /// Which way it points, a unit vector in the tangent plane.
    pub fwd: DVec3,
    /// Metres a second along `fwd`, negative in reverse.
    pub speed: f64,
    /// Height of the wheels over the ground, metres, and how fast that is
    /// changing: a car that drives off a kerb falls off it.
    pub h: f64,
    pub vy: f64,
    pub on_ground: bool,
    /// The radius of the wheels, metres.
    pub foot: f64,
}

/// The car's own OUTLINE in its tangent frame, right and forward in
/// metres: the four corners and the middle of each side.
///
/// A ring of one radius is what the walker is pushed out of walls by and
/// it is wrong for a car, which is four metres long and one and a half
/// wide: a circle round it would be two metres across and could not fit
/// down a 2.75 m lane, and a circle inside it would let the bonnet pass
/// through a wall.
fn outline() -> [DVec2; 8] {
    let (w, l) = (CAR_WIDE * 0.5, CAR_LONG * 0.5);
    [
        DVec2::new(-w, -l),
        DVec2::new(0.0, -l),
        DVec2::new(w, -l),
        DVec2::new(w, 0.0),
        DVec2::new(w, l),
        DVec2::new(0.0, l),
        DVec2::new(-w, l),
        DVec2::new(-w, 0.0),
    ]
}

/// The heights the outline is tested at, over the wheels: the bumper and
/// the roofline. Both are well over a `KERB`, so a kerb is something a
/// car drives up rather than something that stops it.
const HEIGHTS: [f64; 2] = [0.45, 1.25];

impl Driver {
    /// A car taken at `dir` pointing `heading`, standing on the ground.
    pub fn board(field: &dyn Density, bounds: &Bounds, dir: DVec3, heading: DVec3) -> Driver {
        let dir = dir.normalize();
        let fwd = (heading - dir * heading.dot(dir)).normalize_or(DVec3::X);
        Driver {
            dir,
            fwd,
            speed: 0.0,
            h: 0.0,
            vy: 0.0,
            on_ground: true,
            foot: walker::ground(field, bounds, dir, None),
        }
    }

    /// Turn the car about the local up.
    pub fn turn(&mut self, a: f64) {
        let (s, c) = a.sin_cos();
        let right = self.fwd.cross(self.dir);
        self.fwd = (self.fwd * c - right * s).normalize();
    }

    /// Which way the car's right hand side points.
    pub fn right(&self) -> DVec3 {
        self.fwd.cross(self.dir).normalize()
    }

    /// Where the driver's eye is, in the planet's frame.
    pub fn eye(&self) -> DVec3 {
        self.dir * (self.foot + SEAT)
    }

    /// Where a body getting out of it stands: clear of the car, on the
    /// KERB side, which is its RIGHT.
    ///
    /// Traffic here keeps right, so a car rides half a lane right of the
    /// centreline and the pavement is the next 1.5 m out on that same
    /// side: stepping out to the right puts a body 3.7 m from the
    /// centreline, which is the middle of the pavement. The first cut
    /// stepped out to the LEFT, which is the driver's own door and is
    /// also the oncoming lane.
    pub fn kerbside(&self, radius: f64) -> DVec3 {
        (self.dir + self.right() * ((CAR_WIDE * 0.5 + DOOR) / radius)).normalize()
    }

    /// How tight a turn the wheels are asking for at this speed, radians
    /// of yaw a second. The BICYCLE model: a car turns by rolling, so
    /// this is proportional to the speed and a car standing still does
    /// not turn at all.
    pub fn yaw_rate(&self, steer: f64) -> f64 {
        let lock = LOCK / (1.0 + self.speed.abs() / TAPER);
        self.speed * (steer.clamp(-1.0, 1.0) * lock).tan() / WHEELBASE
    }

    /// One frame: the pedals, the wheel, the move and the fall.
    pub fn update(&mut self, field: &dyn Density, bounds: &Bounds, input: &Drive, dt: f64) {
        let dt = dt.min(0.05);
        self.pedals(input, dt);
        self.turn(self.yaw_rate(input.steer) * dt);
        self.fwd = (self.fwd - self.dir * self.fwd.dot(self.dir)).normalize_or(DVec3::X);
        self.roll(field, bounds, dt);
        self.fall(field, bounds, dt);
    }

    /// The throttle, the brake and what a car does with neither down.
    fn pedals(&mut self, input: &Drive, dt: f64) {
        let want = input.throttle.clamp(-1.0, 1.0);
        if input.brake {
            let stop = BRAKE * dt;
            self.speed -= self.speed.clamp(-stop, stop);
        } else if want > 0.0 {
            self.speed = (self.speed + ACCEL * want * dt).min(TOP);
        } else if want < 0.0 {
            self.speed = (self.speed + ACCEL * want * dt).max(-REVERSE);
        } else {
            let coast = DRAG * dt;
            self.speed -= self.speed.clamp(-coast, coast);
        }
    }

    /// The step this frame, pushed out of whatever the car's own outline
    /// meets. Hitting something square on takes the speed off it, and
    /// meeting it at an angle slides along it, which is the same rule the
    /// walker keeps and for the same reason.
    fn roll(&mut self, field: &dyn Density, bounds: &Bounds, dt: f64) {
        if self.speed == 0.0 {
            return;
        }
        let ring = outline();
        let asked = self.speed * dt;
        let to = (self.dir + self.fwd * (asked / bounds.radius)).normalize();
        let got = walker::resolve_body(field, bounds, to, self.foot, self.fwd, &ring, &HEIGHTS);
        // How much of the step survived being pushed out of the walls: a
        // car stopped dead made none of it.
        let made = (got - self.dir).dot(self.fwd) * bounds.radius;
        let g = walker::ground(field, bounds, got, Some(self.foot));
        if g - (self.foot) > CLIMB || made.abs() < asked.abs() * 0.05 {
            self.speed *= CRASH;
            return;
        }
        self.dir = got;
        self.fwd = (self.fwd - got * self.fwd.dot(got)).normalize_or(DVec3::X);
        if made.abs() < asked.abs() * 0.7 {
            self.speed *= 0.85;
        }
        let drop = self.foot - g;
        self.h = if self.on_ground && drop <= CLIMB {
            0.0
        } else {
            drop.max(0.0)
        };
    }

    /// Gravity and the landing. A car has no jump and no ceiling: it
    /// drives off a kerb and comes down, and that is the whole of it.
    fn fall(&mut self, field: &dyn Density, bounds: &Bounds, dt: f64) {
        self.vy -= GRAVITY * dt;
        self.h += self.vy * dt;
        if self.h <= 0.0 {
            self.h = 0.0;
            self.vy = 0.0;
            self.on_ground = true;
        } else {
            self.on_ground = false;
        }
        self.foot = walker::ground(field, bounds, self.dir, Some(self.foot)) + self.h;
    }
}

#[cfg(test)]
mod tests;
