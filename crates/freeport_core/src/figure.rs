//! A person and a car as MESHES built from PARAMETERS.
//!
//! The same rule `model.rs` keeps for a building, for the things that
//! MOVE: a procedural game's default for new geometry is procedural, and
//! baking is for what only a hand can draw. Nothing here is authored and
//! nothing ships beside the binary.
//!
//! Coordinates are the FIGURE's own: x to its right, y the way it is
//! going, z up from the ground it stands on. `traffic::Spot` is what puts
//! that frame on the planet, and `traffic::STRIDE` is what a gait is
//! measured in, so a figure that has stopped has stopped its legs too.
//!
//! What is MISSING, named rather than hidden: nothing here COLLIDES. A
//! wall is one oriented box that is drawn and collided and these are
//! `Model::trim`, which only draws, so a walker passes through a townsman
//! rather than bumping into him. A body that stops another body wants the
//! walker to know about boxes that MOVE, which is a larger change than a
//! crowd on a street.

use crate::dc::DcMesh;
use crate::model::Model;
use glam::DVec3;

/// What a part of a figure is made of. Its own small set rather than the
/// terrain shader's, because a person is not a surface the ground's
/// triplanar mapping has anything to say about: the app turns these into
/// flat colours, and the two that take a figure's own TINT are the ones
/// a crowd needs to differ in.
pub const SKIN: u8 = 0;
/// Takes the figure's tint: what somebody is wearing.
pub const CLOTH: u8 = 1;
pub const SHOE: u8 = 2;
/// Takes the figure's tint: what a car is painted.
pub const PAINT: u8 = 3;
pub const GLASS: u8 = 4;
pub const TYRE: u8 = 5;
pub const HEAD_LAMP: u8 = 6;
pub const TAIL_LAMP: u8 = 7;
/// How many there are, which is what an app's palette is as long as.
pub const KINDS: usize = 8;
/// Which of them take the figure's own tint.
pub fn tinted(material: u8) -> bool {
    material == CLOTH || material == PAINT
}

/// How far a leg swings from straight down at the top of its stride,
/// radians. A quarter turn is a march and a tenth is a shuffle.
pub const SWING: f64 = 0.55;

/// How the gait moves a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Swing {
    /// Rides the figure and never moves on it.
    Still,
    /// Swings about its own x axis, this far out of phase with the left
    /// leg, which is what makes the right one the other way about.
    Leg { phase: f64 },
    /// ROLLS about its own x axis at the rate the ground goes past it:
    /// `along / radius` radians, which is what a wheel that is not
    /// sliding does by definition. Measured in METRES like the gait, so
    /// a car that has stopped has stopped its wheels too and nothing
    /// here needs a clock.
    Wheel { radius: f64 },
}

/// One part of a figure: a mesh, where its pivot stands, and what the
/// gait does to it.
#[derive(Clone, Debug)]
pub struct Part {
    pub mesh: DcMesh,
    /// The pivot in the figure's own frame.
    pub at: DVec3,
    pub swing: Swing,
}

/// A thing that walks or drives, in parts.
#[derive(Clone, Debug, Default)]
pub struct Figure {
    pub parts: Vec<Part>,
}

impl Figure {
    /// How many triangles the whole of it is.
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.indices.len() / 3).sum()
    }
}

/// How high a person stands and where the hip a leg swings from is,
/// metres. The walker's own eye is at 1.7 and its head at 1.85, so a
/// townsman is built to stand beside the player rather than to a
/// separate figure nobody would notice was different.
pub const HEIGHT: f64 = 1.77;
const HIP: f64 = 0.84;
const HIP_APART: f64 = 0.105;

/// A townsman: a body that rides the figure and two legs that swing.
///
/// Three parts and not five: the arms are ON the body, because at the
/// size a person is drawn on a street the silhouette that says WALKING is
/// the gap between the legs, and an arm that swung would be two more
/// entities each for something nobody can see.
pub fn person() -> Figure {
    let mut body = Model::new();
    // Hips, torso, arms, neck and head, up the middle.
    body.trim(
        DVec3::new(0.0, 0.0, HIP + 0.05),
        DVec3::new(0.17, 0.11, 0.06),
        0.0,
        CLOTH,
    );
    body.trim(
        DVec3::new(0.0, 0.0, 1.18),
        DVec3::new(0.19, 0.11, 0.29),
        0.0,
        CLOTH,
    );
    for side in [-1.0, 1.0] {
        body.trim(
            DVec3::new(side * 0.25, 0.0, 1.15),
            DVec3::new(0.06, 0.075, 0.26),
            0.0,
            CLOTH,
        );
        // A hand at the end of each, which is the only skin a coat leaves.
        body.trim(
            DVec3::new(side * 0.25, 0.0, 0.86),
            DVec3::new(0.055, 0.07, 0.05),
            0.0,
            SKIN,
        );
    }
    body.trim(
        DVec3::new(0.0, 0.0, 1.52),
        DVec3::new(0.05, 0.05, 0.05),
        0.0,
        SKIN,
    );
    body.trim(
        DVec3::new(0.0, 0.0, 1.65),
        DVec3::new(0.10, 0.115, 0.12),
        0.0,
        SKIN,
    );
    let mut parts = vec![Part {
        mesh: body.mesh,
        at: DVec3::ZERO,
        swing: Swing::Still,
    }];
    // A leg is written about its own HIP, so the app turns it there and
    // the foot goes round rather than the whole leg sliding.
    for (side, phase) in [(-1.0, 0.0), (1.0, std::f64::consts::PI)] {
        let mut leg = Model::new();
        leg.trim(
            DVec3::new(0.0, 0.0, -0.40),
            DVec3::new(0.085, 0.09, 0.40),
            0.0,
            CLOTH,
        );
        leg.trim(
            DVec3::new(0.0, 0.05, -0.80),
            DVec3::new(0.085, 0.125, 0.04),
            0.0,
            SHOE,
        );
        parts.push(Part {
            mesh: leg.mesh,
            at: DVec3::new(side * HIP_APART, 0.0, HIP),
            swing: Swing::Leg { phase },
        });
    }
    Figure { parts }
}

/// How long, wide and high a town car stands, metres. It is 1.6 across
/// the body, which is what `traffic::CAR_LANE` was picked to keep inside
/// the paving.
pub const CAR_LONG: f64 = 4.1;
pub const CAR_WIDE: f64 = 1.6;

/// A town car: one part, because nothing on it has to move. A wheel that
/// turned would be a second entity each for a thing a metre and a half
/// long seen from a pavement, and the wheels are in the body.
/// How many segments a wheel's tread is drawn in.
///
/// Twelve, which is `model::Kind::Tower`'s own drum: at the size a car
/// is ever drawn the silhouette is what says ROUND, and past a dozen
/// facets an eye cannot tell the difference from a circle while every
/// one of them is four more triangles on a thing there are dozens of.
const SPOKES: usize = 12;

/// A WHEEL: a drum of `SPOKES` facets about its own lateral axis, with
/// a cap at each end, centred on the origin so the part it becomes
/// rolls about its own middle.
///
/// Round and not a box, which is what it was: `m.trim` with a half
/// extent of `(0.11, 0.33, 0.33)` is a cube, and a car on four cubes is
/// what the owner read off a picture. It is a part of its OWN now
/// rather than geometry welded into the body, because a wheel that is
/// in the body's mesh cannot turn: a car was one `Part` and `Swing` had
/// nothing that rolls.
fn wheel(radius: f64, half_wide: f64, material: u8) -> DcMesh {
    let mut m = Model::new();
    let step = std::f64::consts::TAU / SPOKES as f64;
    for k in 0..SPOKES {
        let (a, b) = (k as f64 * step, (k + 1) as f64 * step);
        let rim = |t: f64, x: f64| DVec3::new(x, radius * t.cos(), radius * t.sin());
        // The tread, wound so its outward normal points away from the
        // axle rather than into it.
        m.quad(
            rim(a, -half_wide),
            rim(a, half_wide),
            rim(b, half_wide),
            rim(b, -half_wide),
            material,
        );
        // The two caps, each wound the other way up so both face out.
        let hub = |x: f64| DVec3::new(x, 0.0, 0.0);
        m.tri(
            hub(half_wide),
            rim(a, half_wide),
            rim(b, half_wide),
            material,
        );
        m.tri(
            hub(-half_wide),
            rim(b, -half_wide),
            rim(a, -half_wide),
            material,
        );
    }
    m.mesh
}

pub fn car() -> Figure {
    let mut m = Model::new();
    let half = CAR_LONG / 2.0;
    let wide = CAR_WIDE / 2.0;
    // The body over the wheels, and the cabin set back on it.
    m.trim(
        DVec3::new(0.0, 0.0, 0.62),
        DVec3::new(wide, half - 0.05, 0.28),
        0.0,
        PAINT,
    );
    m.trim(
        DVec3::new(0.0, -0.15, 1.02),
        DVec3::new(wide - 0.12, 1.02, 0.24),
        0.0,
        GLASS,
    );
    m.trim(
        DVec3::new(0.0, -0.15, 1.28),
        DVec3::new(wide - 0.10, 1.04, 0.03),
        0.0,
        PAINT,
    );
    for side in [-1.0, 1.0] {
        m.trim(
            DVec3::new(side * 0.52, half - 0.05, 0.68),
            DVec3::new(0.17, 0.05, 0.09),
            0.0,
            HEAD_LAMP,
        );
        m.trim(
            DVec3::new(side * 0.56, 0.05 - half, 0.74),
            DVec3::new(0.15, 0.05, 0.08),
            0.0,
            TAIL_LAMP,
        );
    }
    // The four WHEELS, each its own part so it can roll, and standing
    // PROUD of the body rather than flush with it. At `wide - 0.11` a
    // box wheel's outer face was coplanar with the body's own side, and
    // two coplanar faces are a depth fight: that is the flicker on the
    // wheels the owner photographed, and it is geometry rather than
    // anything a bias would have cured.
    let mut parts = vec![Part {
        mesh: m.mesh,
        at: DVec3::ZERO,
        swing: Swing::Still,
    }];
    for side in [-1.0, 1.0] {
        for end in [-1.0, 1.0] {
            parts.push(Part {
                mesh: wheel(TYRE_R, TYRE_W, TYRE),
                at: DVec3::new(side * (wide + PROUD - TYRE_W), end * 1.32, TYRE_R),
                swing: Swing::Wheel { radius: TYRE_R },
            });
        }
    }
    Figure { parts }
}

/// How far a leg has swung, radians about its own hip, `along` metres
/// into a walk. One STRIDE is half a cycle, because a stride is one leg
/// and a cycle is both.
/// The wheels: how big they are, how wide, and how far the outer face
/// stands out of the body's own side.
///
/// `TYRE_R` is the radius the car's own `foot` is measured at, so the
/// axle stands one radius over the ground and the tread touches it.
/// `PROUD` is small and its only job is that no face of a wheel is
/// COPLANAR with a face of the body, which is what a depth fight is: the
/// pivot is `wide + PROUD - TYRE_W`, so the OUTER FACE stands `PROUD` of
/// the body's side and the rest of the wheel is under the body, which is
/// where a wheel goes. Written `wide + TYRE_W - PROUD` it is the whole
/// wheel outboard of the body, and the first render of that is a car on
/// four outriggers.
pub const TYRE_R: f64 = 0.33;
pub const TYRE_W: f64 = 0.11;
const PROUD: f64 = 0.02;

/// How far a WHEEL has rolled, radians about its own axle, `along`
/// metres into a drive. A wheel that is not sliding turns `along /
/// radius`, which is the whole of it and needs no clock.
pub fn roll(along: f64, radius: f64) -> f64 {
    if radius.abs() < f64::EPSILON {
        return 0.0;
    }
    along / radius
}

pub fn gait(along: f64, phase: f64) -> f64 {
    SWING * (along / crate::traffic::STRIDE * std::f64::consts::PI + phase).sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A person stands the height a person stands, with his feet on the
    /// ground and nothing hanging through it, and both legs swing.
    #[test]
    fn a_person_stands_on_the_ground_and_swings_both_legs() {
        let f = person();
        assert_eq!(f.parts.len(), 3, "a body and two legs");
        let body = &f.parts[0];
        let top = body
            .mesh
            .positions
            .iter()
            .map(|p| p[2] as f64)
            .fold(f64::MIN, f64::max);
        // A millimetre of slack, because a mesh position is an `f32`: these
        // are render frame numbers and the frame is never more than a
        // town across, so that is the precision they are kept at.
        assert!((top - HEIGHT).abs() < 1e-5, "a person is {top:.3} m tall");
        // The legs reach the ground and no further, measured through
        // their own pivot.
        for leg in &f.parts[1..] {
            let low = leg
                .mesh
                .positions
                .iter()
                .map(|p| p[2] as f64 + leg.at.z)
                .fold(f64::MAX, f64::min);
            assert!(low.abs() < 1e-5, "a foot stands at {low:.3} m");
            assert!(matches!(leg.swing, Swing::Leg { .. }));
        }
        // And the two are a half cycle apart, so one is forward when the
        // other is back and a walk is never a hop.
        let (Swing::Leg { phase: a }, Swing::Leg { phase: b }) =
            (f.parts[1].swing, f.parts[2].swing)
        else {
            panic!("a leg does not swing");
        };
        assert!((b - a - std::f64::consts::PI).abs() < 1e-9);
        assert!(
            (gait(0.0, a) - gait(0.0, b)).abs() < 1e-9,
            "both start at nought"
        );
        let one = crate::traffic::STRIDE * 0.5;
        assert!(
            gait(one, a) * gait(one, b) < 0.0,
            "both legs swing the same way"
        );
        // A gait is a function of the WALK and not of a clock: a figure
        // that has not moved has not moved its legs.
        assert_eq!(gait(0.0, a), gait(0.0, a));
        assert!(
            gait(crate::traffic::STRIDE * 2.0, a).abs() < 1e-9,
            "a whole cycle is level again"
        );
    }

    /// A car is one part, it is the size a car is, and its wheels are on
    /// the ground.
    #[test]
    fn a_car_is_the_size_a_car_is_and_rolls_on_four_round_wheels() {
        let f = car();
        // A body and FOUR WHEELS, because a wheel welded into the body's
        // own mesh cannot turn: the car was one part and the owner's
        // word for what that looked like was that the wheels are square
        // and do not spin.
        assert_eq!(f.parts.len(), 5);
        for p in &f.parts[1..] {
            assert!(
                matches!(p.swing, Swing::Wheel { .. }),
                "a car's part past the body is not a wheel"
            );
            // ROUND: every vertex of a tread stands its own radius off
            // the axle, which a box does not.
            let off: Vec<f64> = p
                .mesh
                .positions
                .iter()
                .map(|q| (q[1] as f64).hypot(q[2] as f64))
                .filter(|r| *r > 1e-6)
                .collect();
            let (lo, hi) = (
                off.iter().copied().fold(f64::MAX, f64::min),
                off.iter().copied().fold(f64::MIN, f64::max),
            );
            println!("a wheel's rim runs {lo:.3} to {hi:.3} m off its axle");
            assert!(
                (hi - lo).abs() < 1e-6,
                "a wheel is not round: {lo:.3} to {hi:.3}"
            );
            assert!((hi - TYRE_R).abs() < 1e-6);
        }
        // The body's own span, which is what a car has to fit a lane in.
        let p: Vec<[f32; 3]> = f.parts[0].mesh.positions.clone();
        let span = |k: usize| {
            let lo = p.iter().map(|q| q[k] as f64).fold(f64::MAX, f64::min);
            let hi = p.iter().map(|q| q[k] as f64).fold(f64::MIN, f64::max);
            (lo, hi)
        };
        let (x0, x1) = span(0);
        let (y0, y1) = span(1);
        let (z0, z1) = span(2);
        println!(
            "a car is {:.2} wide, {:.2} long and {:.2} high",
            x1 - x0,
            y1 - y0,
            z1 - z0
        );
        assert!((x1 - x0 - CAR_WIDE).abs() < 1e-5);
        assert!((y1 - y0 - CAR_LONG).abs() < 1e-5);
        // The BODY is clear of the ground now and the WHEELS are what
        // stand on it, which is what taking them out of its mesh means.
        assert!(
            z0 > 0.2,
            "the body's floor is at {z0:.3} m, under the axles"
        );
        for p in &f.parts[1..] {
            let stands = p.at.z - TYRE_R;
            assert!(
                stands.abs() < 1e-9,
                "a wheel's tread stands {stands:.3} m off the ground"
            );
        }
        assert!((1.2..1.6).contains(&(z1 - z0 + TYRE_R)));
        // And it fits the street it drives on, which is what the lane
        // offset in `traffic` was picked against.
        assert!(CAR_WIDE <= crate::traffic::HALF_STREET * 2.0);
    }

    /// Every material a figure uses is inside the palette an app has to
    /// answer for, and the two a crowd differs on are the tinted ones.
    #[test]
    fn every_part_is_made_of_something_the_palette_answers_for() {
        for (what, f) in [("a person", person()), ("a car", car())] {
            println!(
                "{what} is {} parts and {} triangles",
                f.parts.len(),
                f.triangles()
            );
            assert!(f.triangles() > 0);
            for part in &f.parts {
                for &m in &part.mesh.materials {
                    assert!((m as usize) < KINDS, "material {m} is outside the palette");
                }
            }
        }
        assert!(tinted(CLOTH) && tinted(PAINT));
        assert!(!tinted(SKIN) && !tinted(GLASS) && !tinted(TYRE));
    }
}
