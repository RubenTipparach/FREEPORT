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
use crate::figure::{CAR_HIGH, CAR_LONG, CAR_WIDE};
use crate::fuel::Tank;
use crate::walker::{self, Bounds, Shape};
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
/// What a tyre sliding SIDEWAYS takes off a shove, metres a second a
/// second: six tenths of a gravity, which is a tyre's grip on dry tarmac
/// and is why a car shoved off its line slides a car's length and not a
/// street's.
const SKID: f64 = 6.0;
/// What the tyres take off a SPIN, radians a second a second: a car
/// struck on a quarter swings through most of a right angle and stops
/// turning, rather than pirouetting down the road.
const SPIN_DRAG: f64 = 3.0;
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

/// The BOX a car is to another body: its own outline standing on the
/// ground at `at`, pointing `fwd`, with `up` the local radial there.
///
/// ONE function, because the player's own car, a town's traffic and a
/// car somebody parked on a kerb are all the same box: a second copy
/// would be a car that could be driven through from one side and not the
/// other. It is `figure`'s own three numbers, so the box and the car
/// that is drawn cannot drift, which is this project's rule that a wall
/// is one set of numbers both drawn and collided.
///
/// A car is `Model::trim` where it is DRAWN, because nothing in a town
/// collides with a townsman or with the traffic: this is what a body at
/// the WHEEL meets, and the player is the one thing here that
/// integrates.
pub fn car_box(at: DVec3, fwd: DVec3, up: DVec3) -> crate::field::Block {
    let up = up.normalize_or(DVec3::Y);
    let fwd = (fwd - up * fwd.dot(up)).normalize_or(DVec3::X);
    crate::field::Block {
        centre: at + up * (CAR_HIGH * 0.5),
        half: DVec3::new(CAR_WIDE * 0.5, CAR_LONG * 0.5, CAR_HIGH * 0.5),
        axes: [fwd.cross(up).normalize(), fwd, up],
        material: crate::field::PLATE,
    }
}

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
    /// What is in the TANK, burned off the ground the car makes: a car
    /// that runs dry rolls to a stop and the throttle does nothing until
    /// somebody fills it (`fuel`).
    pub tank: Tank,
    /// Velocity ACROSS the car from being hit (`ram`), in the tangent
    /// plane, metres a second: a car shoved off its line slides on its
    /// tyres until they take it back. Along the car it is `speed`.
    pub shove: DVec3,
    /// Turning from being hit off centre, radians a second, anticlockwise
    /// from above, worn off by the tyres.
    pub spin: f64,
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
            tank: Tank::full(),
            shove: DVec3::ZERO,
            spin: 0.0,
        }
    }

    /// The car's velocity in the tangent plane, metres a second: along
    /// itself and across itself together.
    pub fn velocity(&self) -> DVec3 {
        self.fwd * self.speed + self.shove
    }

    /// HIT: take a change of velocity `dv` (tangent, metres a second)
    /// and a `spin` (radians a second). Along the car it is speed, which
    /// the pedals and the drag then own; across it is a shove the tyres
    /// take back.
    pub fn knock(&mut self, dv: DVec3, spin: f64) {
        let dv = dv - self.dir * dv.dot(self.dir);
        let along = dv.dot(self.fwd);
        self.speed += along;
        self.shove += dv - self.fwd * along;
        self.spin += spin;
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

    /// One frame: out of whatever it is in, the pedals, the wheel, the
    /// move and the fall.
    pub fn update(&mut self, field: &dyn Density, bounds: &Bounds, input: &Drive, dt: f64) {
        let dt = dt.min(0.05);
        // A dry tank is a dead engine: the throttle does nothing either
        // way, and the brake and the wheel still work, because a car
        // that has run out of fuel is still a car rolling.
        let input = Drive {
            throttle: if self.tank.empty() {
                0.0
            } else {
                input.throttle
            },
            ..*input
        };
        self.free(field, bounds);
        self.pedals(&input, dt);
        self.turn(self.yaw_rate(input.steer) * dt);
        self.fwd = (self.fwd - self.dir * self.fwd.dot(self.dir)).normalize_or(DVec3::X);
        let before = self.gone;
        self.roll(field, bounds, dt);
        // Burned off the ground the wheels actually MADE, so a car wedged
        // against a wall burns nothing however hard the throttle is held.
        self.tank.burn(self.gone - before);
        self.skid(field, bounds, dt);
        self.fall(field, bounds, dt);
        self.settle(field, bounds, dt);
    }

    /// The SLIDE and the SPIN a hit left, worn off by the tyres. The
    /// slide is sub stepped like the roll and pushed out of walls like
    /// it, because a car shoved sideways into a building is the same car
    /// that was driven into one; what a wall takes off a slide is all of
    /// it, since a tyre has no crumple across the car.
    fn skid(&mut self, field: &dyn Density, bounds: &Bounds, dt: f64) {
        if self.spin != 0.0 {
            self.turn(self.spin * dt);
            let worn = SPIN_DRAG * dt;
            self.spin -= self.spin.clamp(-worn, worn);
        }
        let rate = self.shove.length();
        if rate <= 0.0 {
            return;
        }
        let ring = outline();
        let shape = Shape {
            ring: &ring,
            heights: &HEIGHTS,
        };
        let asked = rate * dt;
        let steps = (asked / Self::STEP)
            .ceil()
            .clamp(1.0, Self::MOST_STEPS as f64) as usize;
        let step = self.shove * (dt / steps as f64 / bounds.radius);
        for _ in 0..steps {
            let to = (self.dir + step).normalize();
            let got = walker::resolve_body(field, bounds, to, self.foot, self.fwd, shape);
            if walker::swept(field, bounds, (self.dir, got), self.foot, self.fwd, shape) {
                self.shove = DVec3::ZERO;
                break;
            }
            self.gone += (got - self.dir).length() * bounds.radius;
            self.dir = got;
        }
        self.fwd = (self.fwd - self.dir * self.fwd.dot(self.dir)).normalize_or(DVec3::X);
        let worn = SKID * dt;
        let left = (rate - worn).max(0.0);
        self.shove = (self.shove - self.dir * self.shove.dot(self.dir)) * (left / rate);
    }

    /// Push the car OUT of whatever it is already standing in, wherever
    /// that is: **resolve, never "may I"**.
    ///
    /// This is the walker's own oldest rule arriving at the car, and its
    /// absence is what "my car gets stuck in the buildings" was. `roll`
    /// asked whether a step was allowed and, where it was not, RETURNED
    /// without taking the resolved position: a car nosed into a wall
    /// kept whatever overlap it had arrived with for ever, and a car at
    /// rest inside one never moved at all, because `roll` returns at
    /// once when the speed is nought. Nothing in the whole frame could
    /// take a body out of a solid it was already in.
    ///
    /// So the push is a step of its own and it happens FIRST, every
    /// frame, whatever the pedals say. A car that is clear of everything
    /// is not moved by it (`resolve_body` pushes out of nothing when
    /// nothing overlaps), so it costs one ring of samples and changes no
    /// drive that was not already stuck.
    fn free(&mut self, field: &dyn Density, bounds: &Bounds) {
        let ring = outline();
        let shape = Shape {
            ring: &ring,
            heights: &HEIGHTS,
        };
        self.dir = walker::resolve_body(field, bounds, self.dir, self.foot, self.fwd, shape);
        self.fwd = (self.fwd - self.dir * self.fwd.dot(self.dir)).normalize_or(DVec3::X);
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

    /// How far a car may move between two collision tests, metres.
    ///
    /// A wall here is `model::WALL` thick, 0.35 m, and the outline is
    /// tested at its own eight points: a step longer than the wall walks
    /// those points clean THROUGH it, so nothing overlaps at either end
    /// and the car comes down on the far side. At 44.4 m/s a frame of
    /// this world on a software rasteriser is 2.2 m of road and even a
    /// sixtieth is 0.74 m, so the tunnel was the ordinary case rather
    /// than a corner one. A quarter of a metre is comfortably inside the
    /// wall and is nine sub steps at the top speed.
    const STEP: f64 = 0.25;

    /// How many of those a frame may ever take, whatever it asks for.
    /// Nine is the top speed on a slow frame; sixteen is a cap so that a
    /// garbage delta cannot hang the frame, which is this project's own
    /// rule about guarding an expression where it can leave its domain.
    const MOST_STEPS: usize = 16;

    /// The step this frame, pushed out of whatever the car's own outline
    /// meets. Hitting something square on takes the speed off it, and
    /// meeting it at an angle slides along it, which is the same rule the
    /// walker keeps and for the same reason.
    ///
    /// It is SUB STEPPED (`STEP`), so a fast car cannot walk its own
    /// outline through a wall between two frames, and every sub step
    /// KEEPS its resolved position: what is refused is the ground being
    /// too high to climb, and refusing that leaves the car where the
    /// push put it rather than where it asked to be.
    fn roll(&mut self, field: &dyn Density, bounds: &Bounds, dt: f64) {
        if self.speed == 0.0 {
            return;
        }
        let ring = outline();
        let shape = Shape {
            ring: &ring,
            heights: &HEIGHTS,
        };
        let asked = self.speed * dt;
        let steps = (asked.abs() / Self::STEP)
            .ceil()
            .clamp(1.0, Self::MOST_STEPS as f64) as usize;
        let want = asked / steps as f64;
        // The foot the CLIMB is measured from is the frame's own, and
        // the allowance grows with the ground the car has actually made
        // over the whole frame: sub stepping must not hand each step its
        // own kerb, or a cliff would be climbed in nine bites.
        let floor = self.foot;
        let (mut made, mut hit) = (0.0, false);
        let mut top = floor;
        for _ in 0..steps {
            let to = (self.dir + self.fwd * (want / bounds.radius)).normalize();
            let got = walker::resolve_body(field, bounds, to, self.foot, self.fwd, shape);
            // THROUGH a wall rather than into one, which no sub step can
            // fix on its own: a push goes out of the NEAREST face, so a
            // bumper that lands past a thin wall's own mid plane is
            // pushed OUT THE FAR SIDE. Measured on a 0.35 m wall, a car
            // 0.03 m past the middle was shoved 0.15 m forward and
            // carried on at 160 km/h.
            if walker::swept(field, bounds, (self.dir, got), self.foot, self.fwd, shape) {
                hit = true;
                break;
            }
            let step_made = (got - self.dir).dot(self.fwd) * bounds.radius;
            let g = walker::ground(field, bounds, got, Some(self.foot));
            // A kerb's worth of step, plus whatever the GROUND itself
            // rose over the distance the car actually covered.
            if g - floor > CLIMB + (made + step_made).abs() * STEEPEST {
                hit = true;
                break;
            }
            self.gone += (got - self.dir).length() * bounds.radius;
            made += step_made;
            top = g;
            self.dir = got;
            self.fwd = (self.fwd - got * self.fwd.dot(got)).normalize_or(DVec3::X);
            let drop = self.foot - g;
            self.h = if self.on_ground && drop <= CLIMB {
                0.0
            } else {
                drop.max(0.0)
            };
            if step_made.abs() < want.abs() * 0.05 {
                hit = true;
                break;
            }
        }
        if hit {
            self.speed *= CRASH;
        } else if made.abs() < asked.abs() * 0.7 {
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
        //
        // Over the WHOLE frame and never a sub step, because `made` is
        // the ground the car actually covered and `top` is the ground
        // under the last step it actually took: a sub step that was
        // refused climbed nothing and must cost nothing.
        if made.abs() > 1e-6 {
            let grade = ((top - floor) / made).clamp(-HILL, HILL);
            self.speed -= GRAVITY * grade / (1.0 + grade * grade).sqrt() * dt;
            // A car RUNS AWAY downhill and it does not run away for
            // ever: `pedals` clamps what the engine can ask for and
            // gravity is added after it, which is right, so the cap on
            // the other side is what the wind and the wheels take back.
            self.speed = self.speed.clamp(-REVERSE * RUNAWAY, TOP * RUNAWAY);
        }
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
mod knock_tests;
#[cfg(test)]
mod tests;
