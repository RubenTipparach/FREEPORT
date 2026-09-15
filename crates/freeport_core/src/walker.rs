//! The walker: a body on the ground of a planet, on the field and nothing else.
//!
//! A direction on the sphere and a height off the ground, a heading carried
//! as a tangent vector and squared to the local up every frame (the surface
//! walker as a basis, which is the one place that representation is right),
//! velocity with acceleration and friction so a step has weight, a body
//! radius of 35 cm, a step of 60 cm, a head at 1.85 m and a jump that clears
//! a metre. The ground is the first solid under the feet going down from a
//! step above them, the ceiling the first solid going up, and a wall is any
//! solid a ring of points round the body meets between its step and its
//! head, pushed out of along the field's own gradient, sideways only. There
//! is no collider list to keep in step with the picture, because the picture
//! is the collider: a block is walkable the frame it lands. Ported from the
//! mockup's `Walker` and the marched page's rules, numbers and all, and the
//! tests here are the harness's walks, headless.

use crate::field::Density;
use glam::DVec3;

/// Eye height over the feet, metres.
pub const EYE: f64 = 1.7;
/// The body's radius, metres: what a wall is measured against.
pub const RADIUS: f64 = 0.35;
/// The tallest rise walked up without a jump, metres.
pub const STEP: f64 = 0.6;
/// The top of the head over the feet, metres: what a ceiling clamps.
pub const HEAD: f64 = 1.85;
/// Walking and running speed, metres a second.
pub const SPEED: f64 = 5.0;
pub const RUN: f64 = 8.5;
/// How fast the wanted speed is reached, metres a second a second, and
/// how fast a stop is.
const ACCEL: f64 = 28.0;
const FRICTION: f64 = 14.0;
/// A jump's launch speed, metres a second: it clears a metre and a bit.
const JUMP: f64 = 5.3;
const GRAVITY: f64 = 9.81;
/// Ground steeper than this (the cosine of fifty degrees) is a wall unless
/// it tops out within a step of the feet two body widths on.
const STAND: f64 = 0.64;
/// The gradient's step and the clearance a push leaves, metres.
const EPS: f64 = 0.04;
const CLEAR: f64 = 0.02;
/// How deep the feet go under the sea's level before the body floats,
/// metres: the sea holds a walker at wading depth.
pub const WADE: f64 = 1.2;

/// Where the ground can be: the mean radius, which turns metres into angle,
/// and the radii between which the ground is looked for.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub radius: f64,
    pub floor: f64,
    pub top: f64,
    /// The sea's level as a radius, or nought where there is no water to
    /// float in.
    pub sea: f64,
}

/// What the player asked for this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Forward and right, each -1, 0 or 1 (or anything between).
    pub forward: f64,
    pub right: f64,
    pub run: bool,
    pub jump: bool,
    /// Turn about the local up and tilt the look, radians.
    pub turn: f64,
    pub tilt: f64,
}

/// The walker's state. Positions are a direction on the sphere and radii.
#[derive(Clone, Debug)]
pub struct Walker {
    /// The direction of the feet from the planet's centre.
    pub dir: DVec3,
    /// The heading, a unit vector in the tangent plane.
    pub fwd: DVec3,
    /// The look's tilt, radians, up positive.
    pub pitch: f64,
    /// Height of the feet over the ground, metres.
    pub h: f64,
    /// Vertical speed, metres a second.
    pub vy: f64,
    /// Speed forward and right, metres a second.
    pub vel: [f64; 2],
    pub on_ground: bool,
    /// The radius of the feet, metres.
    pub foot: f64,
}

fn solid(field: &dyn Density, dir: DVec3, r: f64) -> bool {
    field.at(dir * r) > 0.0
}

/// The surface between a solid radius `lo` and an air radius `hi`.
fn bisect(field: &dyn Density, dir: DVec3, mut lo: f64, mut hi: f64) -> f64 {
    for _ in 0..7 {
        let m = 0.5 * (lo + hi);
        if solid(field, dir, m) {
            lo = m;
        } else {
            hi = m;
        }
    }
    0.5 * (lo + hi)
}

/// The field's gradient at a point, which climbs into the rock.
fn gradient(field: &dyn Density, p: DVec3) -> DVec3 {
    let d = |axis: DVec3| field.at(p + axis * EPS) - field.at(p - axis * EPS);
    DVec3::new(d(DVec3::X), d(DVec3::Y), d(DVec3::Z))
}

/// The radius of the ground under `dir`: the highest solid no more than a
/// step above the feet, else the first solid going down from a step above
/// them, or from the top of the bounds when the feet are not yet known.
pub fn ground(field: &dyn Density, bounds: &Bounds, dir: DVec3, foot: Option<f64>) -> f64 {
    let (mut r, step) = match foot {
        Some(f) => (f + STEP + 0.01, 0.1),
        None => (bounds.top, 0.5),
    };
    if solid(field, dir, r) {
        // Something is where the step would be: its top is the ground.
        let mut hi = r;
        let mut n = 0;
        while n < 40 && solid(field, dir, hi) {
            hi += 0.1;
            n += 1;
        }
        return bisect(field, dir, hi - 0.1, hi);
    }
    let mut prev = r;
    r -= step;
    while r > bounds.floor {
        if solid(field, dir, r) {
            return bisect(field, dir, r, prev);
        }
        prev = r;
        r -= step;
    }
    bounds.floor
}

/// The radius of the lowest solid over the feet, or infinity.
pub fn ceiling(field: &dyn Density, dir: DVec3, foot: f64) -> f64 {
    let mut r = foot + 0.25;
    for _ in 0..24 {
        r += 0.15;
        if solid(field, dir, r) {
            return r - 0.075;
        }
    }
    f64::INFINITY
}

/// Whether ground at radius `g` under `dir` can be stood on: not steeper
/// than fifty degrees, unless it tops out within a step of the feet two
/// body widths on from `from`, which a kerb's shoulder does and a rail's
/// end, a cliff and a wall do not.
pub fn can_stand(
    field: &dyn Density,
    bounds: &Bounds,
    dir: DVec3,
    g: f64,
    from: DVec3,
    foot: f64,
) -> bool {
    let grad = gradient(field, dir * g);
    let len = grad.length();
    if len < 1e-6 || -grad.dot(dir) / len > STAND {
        return true;
    }
    let stride = dir.distance(from);
    if stride < 1e-12 {
        return false;
    }
    let ahead = (dir + (dir - from) * (2.0 * RADIUS / (stride * bounds.radius))).normalize();
    ground(field, bounds, ahead, Some(foot)) - foot <= STEP
}

/// `to` pushed out of whatever solid a ring of points round the body meets
/// between its step and its head, along the field's own gradient, sideways
/// only: up and down are the other two rules' business.
pub fn resolve(field: &dyn Density, bounds: &Bounds, to: DVec3, foot: f64) -> DVec3 {
    let mut d = to;
    let heights = [foot + STEP + 0.08, foot + 1.1, foot + HEAD - 0.08];
    for _ in 0..3 {
        let pole = if d.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let e = pole.cross(d).normalize();
        let n = d.cross(e).normalize();
        let mut pushed = false;
        for h in heights {
            for a in 0..12 {
                let ang = a as f64 * std::f64::consts::PI / 6.0;
                let p = d * h + e * (ang.cos() * RADIUS) + n * (ang.sin() * RADIUS);
                let dens = field.at(p);
                if dens <= 0.0 {
                    continue;
                }
                let grad = gradient(field, p);
                let len = grad.length();
                if len < 1e-6 {
                    continue;
                }
                // The gradient is a difference over twice the step; the
                // field is a distance where it matters, so the way out is
                // the density over the slope, plus a little.
                let out = dens / (len / (2.0 * EPS)) + CLEAR;
                let mut push = -grad / len;
                push -= d * push.dot(d);
                if push.length() < 0.25 {
                    continue;
                }
                d = (d + push * (out / bounds.radius)).normalize();
                pushed = true;
            }
        }
        if !pushed {
            break;
        }
    }
    d
}

impl Walker {
    /// A walker set down at `dir` facing `heading`, on the ground there.
    pub fn enter(field: &dyn Density, bounds: &Bounds, dir: DVec3, heading: DVec3) -> Walker {
        let dir = dir.normalize();
        let fwd = (heading - dir * heading.dot(dir)).normalize_or(DVec3::X);
        let foot = ground(field, bounds, dir, None);
        Walker {
            dir,
            fwd,
            pitch: -0.05,
            h: 0.0,
            vy: 0.0,
            vel: [0.0, 0.0],
            on_ground: true,
            foot,
        }
    }

    /// Turn the heading about the local up.
    pub fn turn(&mut self, a: f64) {
        let (s, c) = a.sin_cos();
        let right = self.fwd.cross(self.dir);
        self.fwd = (self.fwd * c - right * s).normalize();
    }

    /// Where the eye is.
    pub fn eye(&self) -> DVec3 {
        self.dir * (self.foot + EYE)
    }

    /// Which way the eye looks.
    pub fn look(&self) -> DVec3 {
        (self.fwd * self.pitch.cos() + self.dir * self.pitch.sin()).normalize()
    }

    /// One frame: the look, the speed, the step, the jump and the fall.
    pub fn update(&mut self, field: &dyn Density, bounds: &Bounds, input: &Input, dt: f64) {
        let dt = dt.min(0.05);
        self.turn(input.turn);
        self.pitch = (self.pitch + input.tilt).clamp(-1.45, 1.45);
        self.fwd = (self.fwd - self.dir * self.fwd.dot(self.dir)).normalize_or(DVec3::X);
        self.accelerate(input, dt);
        self.step(field, bounds, input, dt);
        self.rise_and_fall(field, input, dt);
        self.float(bounds);
    }

    /// The sea holds the feet no deeper than `WADE` under its level: past
    /// that the body floats, standing on nothing, and walks.
    fn float(&mut self, bounds: &Bounds) {
        if bounds.sea <= 0.0 {
            return;
        }
        let line = bounds.sea - WADE;
        if self.foot < line {
            self.h += line - self.foot;
            self.foot = line;
            self.vy = 0.0;
            self.on_ground = true;
        }
    }

    /// Accelerate toward the wanted velocity in the tangent plane, and
    /// slow to a stop when nothing is asked for.
    fn accelerate(&mut self, input: &Input, dt: f64) {
        let len = input.forward.hypot(input.right).max(1.0);
        let top = if input.run { RUN } else { SPEED };
        let want = [input.forward / len * top, input.right / len * top];
        let gain = if self.on_ground { ACCEL } else { ACCEL * 0.25 };
        let k = (gain * dt / top).min(1.0);
        for (v, w) in self.vel.iter_mut().zip(want) {
            *v += (w - *v) * k;
        }
        if input.forward == 0.0 && input.right == 0.0 && self.on_ground {
            let f = (1.0 - FRICTION * dt).max(0.0);
            self.vel = [self.vel[0] * f, self.vel[1] * f];
        }
    }

    /// The step this frame: tried whole, then each axis alone, so a wall is
    /// slid along. A step onto ground that rose a little is a step up; ground
    /// that fell by no more than a step is a step down with the feet kept on
    /// it, because airborne on every downslope flickered down every fillet.
    fn step(&mut self, field: &dyn Density, bounds: &Bounds, input: &Input, dt: f64) {
        let right = self.fwd.cross(self.dir).normalize();
        let ground_now = ground(field, bounds, self.dir, Some(self.foot));
        let foot = ground_now + self.h;
        let r = bounds.radius;
        let try_step = |vf: f64, vr: f64| -> Option<(DVec3, f64)> {
            if vf == 0.0 && vr == 0.0 {
                return None;
            }
            let d = (self.dir + self.fwd * (vf * dt / r) + right * (vr * dt / r)).normalize();
            let d = resolve(field, bounds, d, foot);
            // A resolved step that comes back to where it started is a wall
            // square on: not a move.
            if d.distance(self.dir) < 1e-9 {
                return None;
            }
            let g = ground(field, bounds, d, Some(self.foot));
            if g - foot > STEP {
                return None;
            }
            if !can_stand(field, bounds, d, g, self.dir, self.foot) {
                return None;
            }
            Some((d, g))
        };
        let moved = try_step(self.vel[0], self.vel[1])
            .or_else(|| try_step(self.vel[0], 0.0))
            .or_else(|| try_step(0.0, self.vel[1]));
        if let Some((d, g)) = moved {
            self.dir = d;
            self.fwd = (self.fwd - d * self.fwd.dot(d)).normalize_or(DVec3::X);
            let drop = foot - g;
            self.h = if self.on_ground && drop <= STEP {
                0.0
            } else {
                drop.max(0.0)
            };
        } else if input.forward != 0.0 || input.right != 0.0 {
            self.vel = [self.vel[0] * 0.5, self.vel[1] * 0.5];
        }
    }

    /// The jump, gravity, the landing, and the ceiling over the head.
    fn rise_and_fall(&mut self, field: &dyn Density, input: &Input, dt: f64) {
        if input.jump && self.on_ground {
            self.vy = JUMP;
            self.on_ground = false;
        }
        self.vy -= GRAVITY * dt;
        self.h += self.vy * dt;
        if self.h <= 0.0 {
            self.h = 0.0;
            self.vy = 0.0;
            self.on_ground = true;
        } else {
            self.on_ground = false;
        }
        let g = ground(
            field,
            &Bounds {
                radius: 1.0,
                floor: 0.0,
                top: 0.0,
                sea: 0.0,
            },
            self.dir,
            Some(self.foot),
        );
        let c = ceiling(field, self.dir, self.foot);
        let room = c - (g + self.h + HEAD);
        if room < 0.0 {
            self.h = (self.h + room).max(0.0);
            if self.vy > 0.0 {
                self.vy = 0.0;
            }
        }
        self.foot = g + self.h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{Block, Built, Sphere, CONCRETE};

    /// A ball big enough that a flat block on its top is level with the
    /// ground for the length of a walk: on a 20 m ball the ground fell
    /// 0.22 m under a kerb three metres out and the kerb was a wall.
    const R: f64 = 2000.0;

    fn bounds() -> Bounds {
        Bounds {
            radius: R,
            floor: R - 4.0,
            top: R + 8.0,
            sea: 0.0,
        }
    }

    #[test]
    fn the_sea_holds_a_walker_at_wading_depth_and_it_walks_on() {
        let ball = Sphere { radius: R };
        let b = Bounds {
            sea: R + 2.0,
            ..bounds()
        };
        let mut w = Walker::enter(&ball, &b, DVec3::Y, DVec3::X);
        let input = Input {
            forward: 1.0,
            ..Default::default()
        };
        for _ in 0..120 {
            w.update(&ball, &b, &input, 1.0 / 60.0);
        }
        assert!(
            (w.foot - (R + 2.0 - WADE)).abs() < 1e-6,
            "feet at {}",
            w.foot - R
        );
        assert!(w.on_ground);
        assert!(walked(&w) > 6.0, "walked {}", walked(&w));
    }

    /// A block standing on the top of the ball from `ahead` metres along +x
    /// to `ahead + long`, `rise` high over the ball and `wide` across,
    /// sunk a little into it.
    fn block(ahead: f64, long: f64, rise: f64, wide: f64) -> Block {
        Block {
            centre: DVec3::new(ahead + long / 2.0, R + rise / 2.0 - 0.1, 0.0),
            half: DVec3::new(long / 2.0, wide / 2.0, rise / 2.0 + 0.1),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
            material: CONCRETE,
        }
    }

    fn walk(field: &dyn Density, input: Input, frames: usize) -> (Walker, f64) {
        let b = bounds();
        let mut w = Walker::enter(field, &b, DVec3::Y, DVec3::X);
        let mut top_h = 0.0f64;
        for _ in 0..frames {
            w.update(field, &b, &input, 1.0 / 60.0);
            top_h = top_h.max(w.h);
        }
        (w, top_h)
    }

    /// Metres walked along the ground from the top of the ball.
    fn walked(w: &Walker) -> f64 {
        w.dir.angle_between(DVec3::Y) * R
    }

    #[test]
    fn a_walker_stands_on_a_ball_and_walks_a_great_circle() {
        let ball = Sphere { radius: R };
        let (w, _) = walk(
            &ball,
            Input {
                forward: 1.0,
                ..Default::default()
            },
            120,
        );
        assert!((w.foot - R).abs() < 0.02, "the feet at {}", w.foot);
        assert!(w.on_ground);
        let m = walked(&w);
        assert!(m > 8.0 && m < 10.0, "walked {m} m in two seconds");
        assert!((w.vel[0] - SPEED).abs() < 0.01, "at {} m/s", w.vel[0]);
        assert!(w.fwd.dot(w.dir).abs() < 1e-9, "the heading stayed tangent");
        let (w, _) = walk(
            &ball,
            Input {
                forward: 1.0,
                run: true,
                ..Default::default()
            },
            120,
        );
        assert!(walked(&w) > 14.0, "ran {} m", walked(&w));
    }

    #[test]
    fn a_jump_clears_a_metre_and_lands() {
        let ball = Sphere { radius: R };
        let b = bounds();
        let mut w = Walker::enter(&ball, &b, DVec3::Y, DVec3::X);
        let (mut top, mut landed_at) = (0.0f64, None);
        for i in 0..120 {
            let input = Input {
                jump: i == 0,
                ..Default::default()
            };
            w.update(&ball, &b, &input, 1.0 / 60.0);
            top = top.max(w.h);
            if i > 5 && w.on_ground && landed_at.is_none() {
                landed_at = Some(i);
            }
        }
        assert!(top > 1.0 && top < 1.6, "the jump peaked at {top} m");
        let landed = landed_at.expect("the walker came back down");
        assert!(landed > 40 && landed < 80, "landed at frame {landed}");
        assert!((w.foot - R).abs() < 0.02);
    }

    #[test]
    fn a_step_is_climbed_and_a_wall_is_not() {
        let ball = Sphere { radius: R };
        let box_ = block(3.0, 10.0, 0.4, 6.0);
        let kerb = Built {
            ground: &ball,
            blocks: vec![&box_],
        };
        let (w, _) = walk(
            &kerb,
            Input {
                forward: 1.0,
                ..Default::default()
            },
            90,
        );
        assert!(walked(&w) > 3.0, "walked {} m, onto the kerb", walked(&w));
        assert!(
            (w.foot - (R + 0.4)).abs() < 0.03,
            "standing at {} over the ball",
            w.foot - R
        );
        let box_ = block(3.0, 0.8, 1.2, 6.0);
        let wall = Built {
            ground: &ball,
            blocks: vec![&box_],
        };
        let (w, _) = walk(
            &wall,
            Input {
                forward: 1.0,
                ..Default::default()
            },
            120,
        );
        let m = walked(&w);
        assert!(
            m > 2.3 && m < 3.0 - RADIUS + 0.1,
            "stopped at {m} m before a wall at 3.0"
        );
        assert!((w.foot - R).abs() < 0.02, "still on the ball at {}", w.foot);
    }

    #[test]
    fn a_wall_is_slid_along_and_a_ceiling_stops_a_jump() {
        let ball = Sphere { radius: R };
        let box_ = block(3.0, 0.8, 1.2, 30.0);
        let wall = Built {
            ground: &ball,
            blocks: vec![&box_],
        };
        let (w, _) = walk(
            &wall,
            Input {
                forward: 1.0,
                right: 1.0,
                ..Default::default()
            },
            180,
        );
        // Right is +z for a heading of +x under +y.
        let along = w.dir.z * R;
        assert!(along > 4.0, "slid {along} m along the wall");
        assert!(
            w.dir.x * R < 3.0 - RADIUS + 0.1,
            "and stayed before it at {}",
            w.dir.x * R
        );
        // A lintel a stride ahead: a walker set down with no known foot
        // takes the first solid from space and would start on top of it,
        // so it walks under it first, then jumps.
        let lintel = Block {
            centre: DVec3::new(3.5, R + 2.6, 0.0),
            half: DVec3::new(3.0, 3.0, 0.2),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
            material: CONCRETE,
        };
        let roofed = Built {
            ground: &ball,
            blocks: vec![&lintel],
        };
        let b = bounds();
        let mut w = Walker::enter(&roofed, &b, DVec3::Y, DVec3::X);
        let mut top = 0.0f64;
        for i in 0..150 {
            let input = Input {
                forward: if i < 60 { 1.0 } else { 0.0 },
                jump: i == 60,
                ..Default::default()
            };
            w.update(&roofed, &b, &input, 1.0 / 60.0);
            if i > 60 {
                top = top.max(w.h);
            }
        }
        let x = w.dir.x * R;
        assert!(
            x > 0.5 + RADIUS && x < 6.5 - RADIUS,
            "under the lintel at {x}"
        );
        assert!(top < 0.6, "the head met the lintel at {top} m up");
        assert!(w.on_ground);
    }
}
