//! The TARMAC a road is drawn as, once its corridor has been levelled.
//!
//! A road was DATA for a long time: a chain of directions the chart
//! painted and nothing on the ground, so driving between two towns was
//! driving cross country over a route nothing marked. This is the other
//! half, and it is the streets' own cross section rule (`town/street.rs`)
//! at the country's scale: a road has a WIDTH and a surface and a line
//! down the middle of it, and every piece of it is laid on its own patch
//! of the sphere so nothing long enough for the ground to curve under it
//! is placed in one piece.
//!
//! It is not a street. A street has a kerb, two pavements and a crossing
//! at every block; a country road is two lanes of tarmac with a verge,
//! and what it needs that a street does not is to be built a STRETCH at a
//! time, because it is hundreds of kilometres long and a town is eighty
//! metres across.

use crate::field::{PAINT, PLATE, STREET};
use crate::model::Model;
use crate::town::{Frame, LANE};
use glam::DVec3;

/// How high the carriageway stands over the ground it was levelled onto,
/// metres: the depth of the surfacing on its base course.
///
/// Three times a street's five centimetres, and the reason is measured.
/// A street is laid on a town's own plateau, which is one level; a road
/// is laid on a RAMP, and where two pieces meet at a bend the tarmac is
/// mitred, so its outer corner sits a little along the ramp from the
/// station it belongs to and the ramp is at a different height there.
/// Measured over a whole road: 4 mm almost everywhere and up to 0.10 m
/// at the waypoint bends, where the route turns. Fifteen centimetres
/// clears the worst of that and the SHOULDER below hides the step.
pub const LIFT: f64 = 0.15;

/// How wide the shoulder is, metres, and how far UNDER the ground its
/// outer edge is buried.
///
/// A carriageway standing proud of its own verge with nothing between
/// the two is a ribbon floating over a field, and at a grazing angle the
/// ground shows through the gap. It is buried deeper than the tarmac is
/// proud, because the mitre error runs BOTH ways: a shoulder buried by
/// less than that error stands clear of the ground at the outside of a
/// bend, which is the crack it is there to close. The shoulder falls from the tarmac's
/// own edge to a hair under the ground, which is what the verge of a
/// road actually looks like and what a pavement's own slab already does
/// in a town (sunk 15 cm, for the same reason: an underside lying
/// exactly on the ground flecks along its whole length).
const SHOULDER: f64 = 0.7;
const BURIED: f64 = -0.15;

/// How wide the tarmac is either side of the centreline: one lane each
/// way, read off `town::LANE` rather than written again, so a country
/// road and a town's street are the same width of carriageway and a car
/// that fits one fits the other.
pub const HALF: f64 = LANE;

/// How wide a painted line is, metres, and how far in from the tarmac's
/// own edge the two edge lines are set.
const PAINT_W: f64 = 0.12;
const EDGE_IN: f64 = 0.25;

/// How long one dash of the centreline is and how long the gap after it.
const DASH: f64 = 3.0;
const GAP: f64 = 6.0;

/// How far apart the lamps along a lit stretch stand, in PIECES of
/// corridor, and how tall and how far off the carriageway they are.
///
/// A piece is 341 m, so one lamp every third piece is about a kilometre
/// between them: far sparser than a street's, which is what the run in
/// to a town looks like from the road, and sparse enough that a lit
/// approach is a handful of lights rather than a wall of them.
pub const LAMP_EVERY: usize = 3;
const LAMP_H: f64 = 7.0;
const LAMP_OUT: f64 = 0.5;
/// How far a road lamp throws, metres: a good deal further than a
/// building's, because it stands three times as high and there is
/// nothing out here for it to light but the road.
pub const LAMP_REACH: f64 = 26.0;

/// How many pieces of corridor one STRETCH of tarmac is built as.
///
/// A stretch is one mesh, one entity and one frame, the way a town is,
/// and the frame is what decides how long it may be: its vertices are
/// `f32` metres from its own middle, so at sixteen pieces it is 5.5 km
/// across and an `f32` there holds a third of a millimetre. It is also
/// what streams, so it is the grain at which a road arrives and leaves.
pub const STRETCH: usize = 16;

/// The tarmac for a run of corridor points, in a frame of its own:
/// carriageway, a dashed centreline and a solid line down each edge.
///
/// `line` is the refined centreline (`road::centreline`) and `run` the
/// ground under it (`road::survey`), which is the level the corridor was
/// cut to, so the tarmac sits `LIFT` over a plane rather than over
/// whatever the hill used to be.
pub fn stretch(
    frame: &Frame,
    line: &[DVec3],
    run: &[f64],
    open: &[bool],
    lit: &[bool],
    radius: f64,
) -> Model {
    let mut m = Model::new();
    if line.len() < 2 || run.len() != line.len() || open.len() != line.len() {
        return m;
    }
    // Every point in the stretch's own frame, with its own across.
    let at = |k: usize| frame.local(line[k] * (radius + run[k] + LIFT));
    let across = |k: usize| {
        let ahead = if k + 1 < line.len() { k + 1 } else { k };
        let back = k.saturating_sub(1);
        let along = at(ahead) - at(back);
        // The frame's up is its own z, so across is what is square to
        // the road in the frame's own tangent plane.
        DVec3::new(along.y, -along.x, 0.0).normalize_or(DVec3::X)
    };
    let mut along = 0.0;
    for k in 0..line.len() - 1 {
        let run_m = (at(k + 1) - at(k)).length();
        // A piece inside a town's own levelling is the town's to pave,
        // and `road::open` is the same answer the CORRIDOR is cut by, so
        // the tarmac and the ground under it end in the same place.
        if !(open[k] && open[k + 1]) {
            along += run_m;
            continue;
        }
        let (a, b) = (at(k), at(k + 1));
        let (u, v) = (across(k), across(k + 1));
        band(&mut m, (a, u), (b, v), (-HALF, HALF), 0.0, STREET);
        // The two shoulders, falling from the tarmac's edge into the
        // ground, so there is no step for the field to show through.
        for side in [-1.0, 1.0] {
            slope(
                &mut m,
                (a, u),
                (b, v),
                (side * HALF, side * (HALF + SHOULDER)),
                (0.0, BURIED - LIFT),
                STREET,
            );
        }
        // The two edge lines, a hair over the tarmac so they win the
        // depth test, and the centreline's own dashes.
        let edge = HALF - EDGE_IN;
        for side in [-1.0, 1.0] {
            let mid = side * edge;
            band(
                &mut m,
                (a, u),
                (b, v),
                (mid - PAINT_W * 0.5, mid + PAINT_W * 0.5),
                PAINT_LIFT,
                PAINT,
            );
        }
        if (along / (DASH + GAP)).fract() * (DASH + GAP) < DASH {
            band(
                &mut m,
                (a, u),
                (b, v),
                (-PAINT_W * 0.5, PAINT_W * 0.5),
                PAINT_LIFT,
                PAINT,
            );
        }
        // A LAMP on the approach to a town, and none out in the
        // country: `road::lit` is the one place that is decided, and it
        // is the town's own site it is measured from, so a road is lit
        // where a town is near it and dark where nothing is.
        if lit.get(k).copied().unwrap_or(false) && k % LAMP_EVERY == 0 {
            let foot = a + u * (HALF + LAMP_OUT);
            let head = foot + DVec3::Z * LAMP_H;
            // The post only DRAWS, which is this file's own rule that
            // anything a body should pass through is trim: a lamp post
            // is not what stops a car.
            m.trim(
                (foot + head) * 0.5,
                DVec3::new(0.09, 0.09, LAMP_H * 0.5),
                0.0,
                PLATE,
            );
            m.lamp(head);
        }
        along += run_m;
    }
    m
}

/// How far a marking stands over the tarmac, metres: the streets' own
/// four millimetres, which is what stops a painted line flickering
/// against the road it is painted on.
const PAINT_LIFT: f64 = 0.004;

/// One band of the cross section between two stations: a quad from
/// `lo` to `hi` across, `up` over the surface.
fn band(
    m: &mut Model,
    a: (DVec3, DVec3),
    b: (DVec3, DVec3),
    (lo, hi): (f64, f64),
    up: f64,
    material: u8,
) {
    let z = DVec3::Z * up;
    m.quad(
        a.0 + a.1 * lo + z,
        a.0 + a.1 * hi + z,
        b.0 + b.1 * hi + z,
        b.0 + b.1 * lo + z,
        material,
    );
}

/// How many stretches a run of `points` centreline points is built in.
pub fn count(points: usize) -> usize {
    points.saturating_sub(1).div_ceil(STRETCH)
}

/// Which centreline points the `k`th stretch covers, ends included, so
/// two neighbouring stretches SHARE a point and their tarmac meets.
pub fn span(k: usize, points: usize) -> std::ops::Range<usize> {
    let from = (k * STRETCH).min(points);
    let to = (from + STRETCH + 1).min(points);
    from..to
}

/// The frame a stretch is built and placed in: its own middle, standing
/// on the level the corridor was cut to there.
///
/// Its own middle and never the road's, which is the chunk local rule at
/// a road's scale: a stretch is 5.5 km long, so an `f32` from its middle
/// holds a third of a millimetre, and from the far end of a road it
/// would hold nothing at all.
pub fn frame(line: &[DVec3], run: &[f64], radius: f64) -> Frame {
    let k = line.len() / 2;
    let dir = line.get(k).copied().unwrap_or(DVec3::Y);
    let base = radius + run.get(k).copied().unwrap_or(0.0);
    let (east, north) = crate::town::frame_at(dir);
    Frame {
        dir,
        east,
        north,
        base,
    }
}

/// A band whose two edges stand at different heights: the shoulder.
fn slope(
    m: &mut Model,
    a: (DVec3, DVec3),
    b: (DVec3, DVec3),
    (lo, hi): (f64, f64),
    (up_lo, up_hi): (f64, f64),
    material: u8,
) {
    let (zl, zh) = (DVec3::Z * up_lo, DVec3::Z * up_hi);
    // Wound so the outward face is up whichever side it is on.
    let (p, q, r, s) = (
        a.0 + a.1 * lo + zl,
        a.0 + a.1 * hi + zh,
        b.0 + b.1 * hi + zh,
        b.0 + b.1 * lo + zl,
    );
    if lo < hi {
        m.quad(p, q, r, s, material);
    } else {
        m.quad(s, r, q, p, material);
    }
}
