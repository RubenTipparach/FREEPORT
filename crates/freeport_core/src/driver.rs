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

/// How fast a car goes, metres a second: 160 km/h flat out and a crawl
/// backwards, because nobody reverses fast.
///
/// It was 16 m/s, which is 58 km/h, and that is a town car's own speed
/// rather than a car's: a road between two settlements here is tens of
/// kilometres and at 58 km/h the nearest one to the port is ten minutes
/// away. 44.4 m/s is 160 km/h, which is what the owner asked for and is
/// what the country roads in this world are for.
///
/// `ACCEL` is unchanged at 5.5 m/s^2, so nought to a hundred km/h is
/// about five seconds and the top is reached in eight, which is a car
/// rather than a rocket. What it DOES change is the lock taper: `TAPER`
/// is the speed the steering is given up over, so at nearly three times
/// the top speed the wheel is a third of the lock far earlier in the
/// range, which is the right way round, because a car cornering hard at
/// 160 km/h would be pulling gravities it has no tyres for.
pub const TOP: f64 = 44.4;
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
/// The steepest GROUND it drives up, rise over run.
///
/// It is `walker::STAND` said as a GRADE rather than a second number:
/// `sqrt(1 - STAND^2) / STAND`, fifty degrees, and
/// `a_car_climbs_exactly_what_a_walker_can_stand_on` holds the two in
/// step. A body drives over what it walks over, and a slope past this
/// is a cliff to both.
///
/// A step was refused on `CLIMB` alone, which is a rule in METRES that
/// is really a rule in FRAMES: at sixty a second a car covers 0.27 m and
/// a one in two hill rises 0.13 under it, and at the twentieth a
/// software rasteriser runs at it covers 0.8 and rises 0.4, so the same
/// hill the same car climbed was a wall. Measured, ten seconds of
/// twentieths up one in two went 64.8 m against 136.9.
const STEEPEST: f64 = 1.200_5;
/// What is left of the speed when the car hits something square on.
const CRASH: f64 = 0.15;
const GRAVITY: f64 = 9.81;
/// The steepest grade the hill term is measured at, rise over run.
///
/// It is a GUARD and not a limit on what a car may climb: `resolve_body`
/// has already refused anything past `walker::STAND`, so what reaches
/// here is ground a body can stand on, and this only stops one frame
/// that made a hand of ground over a step from reading as a cliff and
/// taking the whole of the speed off in one go.
const HILL: f64 = 2.0;
/// How far past its own powered top speed gravity may carry a car
/// downhill, as a multiple of it. A car does coast past what its engine
/// can hold on a long descent, and it does not do so without limit.
const RUNAWAY: f64 = 1.5;

/// The steepest grade a car's own drive can HOLD against gravity, rise
/// over run.
///
/// DERIVED and never chosen: the engine is `ACCEL` along the road and
/// gravity takes `GRAVITY * sin(theta)` back, so the two balance at
/// `sin(theta) = ACCEL / GRAVITY` and the grade there is
/// `ACCEL / sqrt(GRAVITY^2 - ACCEL^2)`, which is **0.677, a one in
/// 1.48**. Past it a car rolls backwards however hard the throttle is
/// held, which is what a car does and is the whole of "torque against
/// momentum": nothing refuses the climb, gravity simply wins it.
///
/// A highway is built at `road::STEEPEST`, which is a TENTH of this, so
/// a car tops out on every road on the body.
pub fn holds() -> f64 {
    ACCEL / (GRAVITY * GRAVITY - ACCEL * ACCEL).sqrt()
}
/// How long the bodywork takes to lay itself on a new slope, seconds.
/// Short, because a car on its springs settles in about this, and a car
/// that snapped would flick over every seam in the mesh.
const LEAN: f64 = 0.12;
/// How far off the nose a place has to be for FULL lock, radians. A
/// quarter turn: anything further round and the wheel is hard over
/// anyway, and anything nearer eases off, so a car does not saw at the
/// wheel about its own line.
const AIM: f64 = std::f64::consts::FRAC_PI_4;
/// The driver's eye over the road, metres, and where a body gets out.
pub const SEAT: f64 = 1.15;
pub const DOOR: f64 = 1.5;
/// How near a car a body has to be to open its door, metres, measured
/// from the car's own MIDDLE.
///
/// Eight and not four, which is what a scripted theft measured: a car is
/// 4.1 m long, so four metres from its middle is a hand's width from its
/// own bodywork, and a headless run that walked up a street and pressed
/// E took nothing at all because no car happened to be that close on the
/// frame it asked. Eight is a stride or two off the kerb, which is what
/// walking up to a car is.
pub const REACH: f64 = 8.0;

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
    /// How far this car has actually COME, metres: the ground that has
    /// gone past its wheels, which is what a wheel turns on.
    ///
    /// The arc the car SWEPT and never its own `speed` integrated: a
    /// crash scales the speed down rather than stopping the car, so the
    /// dial says what the engine is asking for and the wheels of a
    /// wedged car would spin on bare tarmac. It is the same distinction
    /// `drive::Auto` already makes to tell a stuck car from a slow one.
    pub gone: f64,
    /// Which way is UP for the BODYWORK: the ground's own normal under
    /// the four wheels, eased.
    ///
    /// Not `dir`, which is up for the PLANET, and the difference is the
    /// whole of it: on a hillside the radial is not the surface normal,
    /// so a car drawn off `dir` sits dead level while the hill falls
    /// away under it. The owner read that off a picture of a car parked
    /// on a slope.
    ///
    /// It is a fact about the GROUND, so it is the core's and not a
    /// thing the app works out to draw with: what a body is standing on
    /// is the same question the walker and the collider already ask the
    /// field, and an app that derived its own would be a second answer.
    pub lean: DVec3,
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
            gone: 0.0,
            lean: dir,
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

    /// Which way the wheel has to go to point the car at `goal`, -1 to
    /// 1, and nought once it is aimed there.
    ///
    /// A bearing error rather than a heading difference, because a
    /// heading is a tangent vector and two of them at two places on a
    /// sphere are not in the same plane: what a driver actually reads
    /// is how far off his own nose the place he is going sits.
    pub fn toward(&self, goal: DVec3) -> f64 {
        let want = (goal - self.dir * goal.dot(self.dir)).normalize_or_zero();
        if want.length_squared() < 0.5 {
            return 0.0;
        }
        // Left is positive, which is what `steer` is: the angle off the
        // nose, measured about the local up.
        let off = want.dot(-self.right()).atan2(want.dot(self.fwd));
        (off / AIM).clamp(-1.0, 1.0)
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
        self.settle(field, bounds, dt);
    }

    /// Lay the bodywork on the ground the WHEELS are standing on.
    ///
    /// The four wheels and not the field's gradient at one point: a
    /// gradient is the slope of a hand's width of ground and a car is
    /// 4.1 m by 1.6, so a gradient would pitch the whole car over every
    /// pebble the mesher drew, and what a car actually rests on is the
    /// plane through its own contact patches. The normal is the cross of
    /// the two DIAGONALS, which is the symmetric answer for a quad whose
    /// four corners are not coplanar, and every quad on ground like this
    /// is one.
    ///
    /// Eased on `LEAN`, a time constant in SECONDS like the chase
    /// camera's `SWING`, so the lean takes the same wall time at twenty
    /// frames a second as at a hundred and twenty; a share of a frame
    /// would be a different car on every machine.
    fn settle(&mut self, field: &dyn Density, bounds: &Bounds, dt: f64) {
        let (w, l) = (CAR_WIDE * 0.5, CAR_LONG * 0.5);
        let right = self.right();
        let corner = |x: f64, y: f64| {
            let d = (self.dir + (right * x + self.fwd * y) / bounds.radius).normalize();
            d * walker::ground(field, bounds, d, Some(self.foot))
        };
        let (fl, fr) = (corner(-w, l), corner(w, l));
        let (rl, rr) = (corner(-w, -l), corner(w, -l));
        let want = (fr - rl).cross(fl - rr).normalize_or(self.dir);
        // Out of the ground and never into it, whichever way the corners
        // happened to be ordered on this patch of sphere.
        let want = if want.dot(self.dir) < 0.0 {
            -want
        } else {
            want
        };
        let t = 1.0 - (-dt / LEAN).exp();
        self.lean = (self.lean + (want - self.lean) * t).normalize_or(self.dir);
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
        self.gone += (got - self.dir).length() * bounds.radius;
        let g = walker::ground(field, bounds, got, Some(self.foot));
        // A kerb's worth of step, plus whatever the GROUND itself rose
        // over the distance the car actually covered.
        let allowed = CLIMB + made.abs() * STEEPEST;
        if g - (self.foot) > allowed || made.abs() < asked.abs() * 0.05 {
            self.speed *= CRASH;
            return;
        }
        self.dir = got;
        self.fwd = (self.fwd - got * self.fwd.dot(got)).normalize_or(DVec3::X);
        if made.abs() < asked.abs() * 0.7 {
            self.speed *= 0.85;
        }
        // And the HILL takes speed off, which is the whole of what a
        // hill does to a car: torque against momentum, and nothing
        // that refuses the climb outright.
        //
        // There was NO gravity along the slope at all, so a car held
        // exactly its speed up a one in one and down it: the owner's
        // "cars should be able to drive up hills, looks like you have
        // some code blocking it" is the other half of that, because
        // what a car actually does on a hill it cannot climb is SLOW
        // and stop, not be refused a step. `GRAVITY * sin(theta)`
        // along the way it is going is that, and it is signed off the
        // travel rather than off the ground, so gravity opposes a climb
        // whether the car is going forward up it or backing up it.
        //
        // Against `ACCEL` (5.5 m/s^2) it is worth nothing on a road and
        // everything on a cliff: a highway's own seven per cent costs
        // 0.69 m/s^2, so a car holds its top speed up one; a one in two
        // costs 4.4 and the car climbs it slowly; a one in one costs
        // 6.9 and the car stalls on it, which is a hill a car does not
        // get up and is the right answer rather than a wall.
        let rise = g - self.foot;
        if made.abs() > 1e-6 {
            let grade = (rise / made).clamp(-HILL, HILL);
            self.speed -= GRAVITY * grade / (1.0 + grade * grade).sqrt() * dt;
            // A car RUNS AWAY downhill and it does not run away for
            // ever: `pedals` clamps what the engine can ask for and
            // gravity is added after it, which is right, so the cap on
            // the other side is what the wind and the wheels take back.
            self.speed = self.speed.clamp(-REVERSE * RUNAWAY, TOP * RUNAWAY);
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
