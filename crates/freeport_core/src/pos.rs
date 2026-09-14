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
