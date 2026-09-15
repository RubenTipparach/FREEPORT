//! Where a thing IS, in metres, in a frame that never loses precision.
//!
//! A planet a thousand kilometres across and a bolt on a ship's hull are both
//! positions in this world, and an `f32` cannot hold both: at ten thousand
//! kilometres from the origin a single precision float steps in whole metres
//! (`f32_step`), so a ship parked at that range jitters by its own length and
//! two bolts a hand apart are the same number. The world frame is therefore
//! `f64`, always, for anything that persists or is sent, and the RENDERER's
//! `f32` frame is measured from a floating origin that follows the eye. That
//! is tenebris's rule and big_space's, kept here in the core so the app cannot
//! get it differently from the tests.

use glam::{DVec3, Vec3};

/// A position in the world frame, in metres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldPos(pub DVec3);

/// How far the eye may drift from the origin before the origin follows it, in
/// metres. Two kilometres keeps every `f32` within a quarter of a millimetre
/// (`f32_step(2000.0)` is about 0.00012), which is under anything a renderer
/// can show.
pub const REBASE_RADIUS: f64 = 2_000.0;

/// The grid the origin snaps to when it moves, in metres. Snapping rather than
/// following exactly means an origin is one of a countable set of positions, so
/// a rebase is a deterministic function of where the eye is and never of the
/// frame it happened on.
pub const ORIGIN_CELL: f64 = 1_000.0;

/// The floating origin: the world position the renderer's `f32` frame is
/// measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Origin {
    pub at: DVec3,
}

impl Origin {
    /// A world position in the renderer's frame.
    pub fn local(&self, p: WorldPos) -> Vec3 {
        (p.0 - self.at).as_vec3()
    }

    /// A renderer position back in the world frame.
    pub fn world(&self, local: Vec3) -> WorldPos {
        WorldPos(self.at + local.as_dvec3())
    }

    /// Move the origin to the eye's cell if the eye has drifted past
    /// `REBASE_RADIUS`, and say whether it moved. The app rebases every
    /// transform it holds when this returns true, and on no other frame.
    pub fn follow(&mut self, eye: WorldPos) -> bool {
        if (eye.0 - self.at).length() <= REBASE_RADIUS {
            return false;
        }
        self.at = (eye.0 / ORIGIN_CELL).round() * ORIGIN_CELL;
        true
    }
}

/// The distance between one `f32` and the next at magnitude `x`, in the same
/// unit as `x`. This is the number every argument about precision comes down
/// to, so it is a function rather than a figure in a comment.
pub fn f32_step(x: f64) -> f64 {
    let f = x.abs() as f32;
    if f == 0.0 {
        return f32::from_bits(1) as f64;
    }
    (f32::from_bits(f.to_bits() + 1) - f) as f64
}

/// `normalize(anchor + step) - anchor` for a UNIT `anchor` and a small
/// `step`, computed so it can be done in f32 without losing the answer.
///
/// This is how a vertex shader draws at a precision it does not have. A
/// point on a planet is `direction * radius`, and at a thousand
/// kilometres an f32 direction carries six hundredths of a micron of
/// error, which is TWELVE CENTIMETRES on the ground: the ground
/// quantises, and it shifts again every time the origin rebases. The way
/// out is never to form the direction at all. Every vertex is an OFFSET
/// from one anchor the CPU works out in f64, so the shader multiplies the
/// radius by a SMALL number that is accurate rather than by a number near
/// one that is not.
///
/// Doing that naively (`normalize(a + q) - a`) throws the accuracy away
/// again, because it subtracts two nearly equal vectors. So the
/// subtraction is done in closed form instead:
/// `normalize(a + q) - a = a * (k - 1) + q * k` with
/// `k = 1 / sqrt(1 + s)` and `s = 2 a.q + q.q`, and `k - 1` is written as
/// `-s / (sqrt(1+s) * (1 + sqrt(1+s)))`, which has no cancellation in it
/// at all. `a_small_step_keeps_its_metres_at_a_thousand_kilometres`
/// measures what that buys, and `frame.wgsl` is the transcription.
pub fn unit_offset(anchor: DVec3, step: DVec3) -> DVec3 {
    let s = 2.0 * anchor.dot(step) + step.dot(step);
    let root = (1.0 + s).max(0.0).sqrt();
    if root <= 0.0 {
        return -anchor;
    }
    let k = 1.0 / root;
    // `k - 1`, without ever forming the difference of two ones.
    let g = -s / (root * (1.0 + root));
    anchor * g + step * k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_f32_at_ten_thousand_kilometres_cannot_tell_a_millimetre() {
        let a = 1.0e7_f64;
        assert_eq!(a as f32, (a + 0.001) as f32);
        assert!(f32_step(a) >= 0.5, "step at 10,000 km is {}", f32_step(a));
    }

    #[test]
    fn rebased_it_can() {
        let origin = Origin {
            at: DVec3::new(1.0e7, 0.0, 0.0),
        };
        let a = origin.local(WorldPos(DVec3::new(1.0e7 + 1.0, 2.0, 3.0)));
        let b = origin.local(WorldPos(DVec3::new(1.0e7 + 1.001, 2.0, 3.0)));
        assert_ne!(a, b);
        assert!((b.x - a.x - 0.001).abs() < 1e-5);
    }

    #[test]
    fn follow_moves_only_past_the_radius_and_snaps_to_the_cell() {
        let mut origin = Origin::default();
        assert!(!origin.follow(WorldPos(DVec3::new(REBASE_RADIUS * 0.9, 0.0, 0.0))));
        assert_eq!(origin.at, DVec3::ZERO);
        let eye = WorldPos(DVec3::new(2_600.0, -1_499.0, 12.0));
        assert!(origin.follow(eye));
        assert_eq!(origin.at, DVec3::new(3_000.0, -1_000.0, 0.0));
        assert!(!origin.follow(eye));
    }

    #[test]
    fn a_small_step_keeps_its_metres_at_a_thousand_kilometres() {
        // What a vertex shader has to do on a big planet, done three
        // ways: in f64 (the answer), the naive f32 way a shader reaches
        // for first, and `unit_offset` in f32. The naive one is what puts
        // a planet's ground on a twelve centimetre grid.
        let radius = 1.0e6_f64;
        let anchor = DVec3::new(0.31, 0.62, 0.72).normalize();
        let (east, north) = (
            anchor.cross(DVec3::Y).normalize(),
            anchor.cross(anchor.cross(DVec3::Y).normalize()).normalize(),
        );
        let (mut naive, mut stable) = (0.0_f64, 0.0_f64);
        for i in 0..40 {
            for j in 0..40 {
                // A step of up to a couple of hundred metres, as a hex
                // window or a near leaf is.
                let metres = DVec3::new(i as f64 - 20.0, 0.0, j as f64 - 20.0) * 8.0;
                let step = (east * metres.x + north * metres.z) / radius;
                let exact = unit_offset(anchor, step) * radius;
                // The naive way: two unit vectors in f32, subtracted.
                let a32 = anchor.as_vec3();
                let p32 = (anchor + step).as_vec3().normalize();
                let got = ((p32 - a32).as_dvec3()) * radius;
                naive = naive.max((got - exact).length());
                // The stable way, every step of it in f32.
                let mine = unit_offset_f32(a32, step.as_vec3()).as_dvec3() * radius;
                stable = stable.max((mine - exact).length());
            }
        }
        println!("at {radius} m: naive {naive:.4} m, stable {stable:.6} m");
        assert!(naive > 0.05, "the naive way was not the problem: {naive} m");
        assert!(stable < 1.0e-3, "the stable way drifted {stable} m");
        assert!(stable * 100.0 < naive, "{stable} against {naive}");
    }

    /// `unit_offset` with every step of it taken in f32, which is what a
    /// shader does.
    fn unit_offset_f32(anchor: glam::Vec3, step: glam::Vec3) -> glam::Vec3 {
        let s = 2.0 * anchor.dot(step) + step.dot(step);
        let root = (1.0 + s).max(0.0).sqrt();
        let k = 1.0 / root;
        let g = -s / (root * (1.0 + root));
        anchor * g + step * k
    }

    #[test]
    fn an_offset_is_the_difference_it_says_it_is() {
        let anchor = DVec3::new(-0.2, 0.9, 0.35).normalize();
        for step in [
            DVec3::ZERO,
            DVec3::new(1.0e-6, 0.0, 2.0e-6),
            DVec3::new(0.01, -0.02, 0.03),
            DVec3::new(0.4, 0.3, -0.2),
        ] {
            let want = (anchor + step).normalize() - anchor;
            let got = unit_offset(anchor, step);
            assert!((want - got).length() < 1e-12, "{want} against {got}");
        }
    }

    #[test]
    fn relative_positions_survive_a_rebase() {
        let mut origin = Origin {
            at: DVec3::new(5.0e6, 5.0e6, 5.0e6),
        };
        let p = WorldPos(DVec3::new(5.0e6 + 3_100.0, 5.0e6 + 7.25, 5.0e6 - 0.5));
        let q = WorldPos(DVec3::new(5.0e6 + 3_101.5, 5.0e6 + 7.0, 5.0e6 + 0.5));
        let before = origin.local(q) - origin.local(p);
        assert!(origin.follow(p));
        let after = origin.local(q) - origin.local(p);
        assert!((before - after).length() < 1e-3, "{before} vs {after}");
        assert!((origin.local(p).length() as f64) < ORIGIN_CELL);
    }

    #[test]
    fn world_and_local_are_inverses_near_the_origin() {
        let origin = Origin {
            at: DVec3::new(-4.0e5, 1.0e3, 9.0e6),
        };
        let p = WorldPos(origin.at + DVec3::new(12.5, -3.0, 700.0));
        let back = origin.world(origin.local(p));
        assert!((back.0 - p.0).length() < 1e-3);
    }
}
