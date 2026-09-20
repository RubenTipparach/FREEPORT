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

use crate::field::{CONCRETE, PAINT, STREET};
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
pub const PAINT_W: f64 = 0.12;
const EDGE_IN: f64 = 0.25;

/// How long one dash of the centreline is and how long the gap after it.
pub const DASH: f64 = 3.0;
pub const GAP: f64 = 6.0;

/// How far apart the lamps along a lit stretch stand, METRES, and how
/// tall and how far off the carriageway they are.
///
/// In metres and not in PIECES, which is what it was and is the same
/// mistake the centreline's dashes made: a piece is 341 m, so one lamp
/// every third piece is one lamp a kilometre, and the whole lit approach
/// to the port came out as a single light. The lamps ALTERNATE sides, so
/// forty five metres apart along the road is ninety on each side, which
/// is what a staggered pair on a two lane road is.
pub const LAMP_EVERY: f64 = 45.0;
const LAMP_H: f64 = 7.0;
const LAMP_OUT: f64 = 0.5;

/// How many lamps a stretch is allowed for INDEXING, which is what makes
/// a lamp's place along the whole road a number.
///
/// A stretch is 5.5 km and a lamp stands every 45 m, so 128 is well over
/// what any stretch can hold and the `k * LAMPS + i` a stretch's lamps
/// are numbered by cannot reach into the next stretch's range. A road's
/// stretches STREAM, so a lamp's identity has to be a fact about the
/// road rather than a slot in what happens to be standing.
pub const LAMPS: usize = 128;
/// How far a road lamp throws, metres: a good deal further than a
/// building's, because it stands three times as high and there is
/// nothing out here for it to light but the road.
pub const LAMP_REACH: f64 = 26.0;

/// How many pieces of corridor one STRETCH of tarmac is built as.
///
/// A stretch is one mesh, one entity and one frame, the way a town is,
/// and the frame is what decides how long it may be: its vertices are
/// `f32` metres from its own middle, so at sixty four pieces of
/// `PIECE` it is 5.4 km across and an `f32` there holds a third of a
/// millimetre. It is also what streams, so it is the grain at which a
/// road arrives and leaves.
///
/// SIXTY FOUR and not sixteen, because the piece is a quarter of what it
/// was: a stretch is a LENGTH of road and not a count of pieces, and
/// left at sixteen every road on the body would have arrived and left in
/// 1.4 km bites, which is four times the meshes and four times the
/// entities for the same tarmac.
pub const STRETCH: usize = 64;

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
        // A piece outside every town's levelling is the road's to pave,
        // and `road::open` is the same answer the CORRIDOR is cut by, so
        // the tarmac and the ground under it end in the same place. What
        // carries it the rest of the way is `road::slip`, which is
        // geometry that reads the ground and not a wider mask: a mask
        // laid tarmac at the road's own baked profile over a town's flat
        // plateau and floated 1.790 m above it.
        if !(open[k] && open[k + 1]) {
            along += run_m;
            continue;
        }
        let (t0, t1) = (0.0, 1.0);
        let part = |t: f64| {
            (
                at(k).lerp(at(k + 1), t),
                across(k).lerp(across(k + 1), t).normalize_or(across(k)),
            )
        };
        let ((a, u), (b, v)) = (part(t0), part(t1));
        // The dashes keep the phase of the WHOLE piece, so a road whose
        // last piece starts part way along does not restart its
        // markings at the junction.
        let (from, laid) = (along + run_m * t0, (b - a).length());
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
        dashes(&mut m, (a, u), (b, v), from, laid);
        // The LAMPS on the approach to a town, and none out in the
        // country: `road::lit` is the one place that is decided, and it
        // is the town's own site it is measured from, so a road is lit
        // where a town is near it and dark where nothing is.
        if lit.get(k).copied().unwrap_or(false) {
            posts(&mut m, (a, u), (b, v), from, laid);
        }
        along += run_m;
    }
    m
}

/// How far a marking stands over the tarmac, metres: the streets' own
/// four millimetres, which is what stops a painted line flickering
/// against the road it is painted on.
const PAINT_LIFT: f64 = 0.004;

/// The centreline's DASHES along one piece of corridor.
///
/// A dash is three metres and a PIECE is three hundred and forty one, so
/// the dashes cannot be a property of the piece. Asked once a piece
/// (`(along / (DASH + GAP)).fract() < ...`) the line came out as 341 m of
/// solid paint and then 682 m of nothing, which is not a dashed line, it
/// is a broken one, and the first picture of a road showed two edge lines
/// and no middle at all because the piece under the camera fell in a gap.
/// The piece is walked in `DASH + GAP` steps instead and the painted part
/// of each step is its own quad, with the station and the across
/// interpolated to where it falls.
///
/// `along` is measured from the start of the STRETCH rather than of the
/// road, so the pattern restarts every 5.5 km and one dash at a stretch
/// seam is short. A stretch is what a road is built and streamed in and
/// it knows nothing of the pieces before it; a global phase would mean
/// walking the whole road to lay any of it, for one short dash in five
/// thousand.
fn dashes(m: &mut Model, a: (DVec3, DVec3), b: (DVec3, DVec3), along: f64, run_m: f64) {
    if !(run_m.is_finite() && along.is_finite() && run_m > 0.0) {
        return;
    }
    let cycle = DASH + GAP;
    let at = |t: f64| (a.0.lerp(b.0, t), a.1.lerp(b.1, t).normalize_or(a.1));
    // The cycle boundary at or before this piece starts, so a dash that
    // straddles the join is drawn by both pieces and meets itself.
    let first = along - along.rem_euclid(cycle);
    for i in 0..stations(run_m, cycle) {
        let s = first + i as f64 * cycle;
        let (lo, hi) = (
            (s.max(along) - along) / run_m,
            ((s + DASH).min(along + run_m) - along) / run_m,
        );
        if hi > lo {
            let (p, q) = (at(lo), at(hi));
            band(m, p, q, (-PAINT_W * 0.5, PAINT_W * 0.5), PAINT_LIFT, PAINT);
        }
    }
}

/// How many stations of a given spacing a piece of road is walked at,
/// and a CAP on it.
///
/// A piece is `road::PIECE` and the closest spacing anything here is
/// laid at is a dash's nine metres, so forty is the real count. Four
/// thousand is a piece a hundred times longer than one can be, and it is
/// there because a loop stepping a fixed distance over a length it was
/// handed is a loop a garbage line hangs the mesher with: a hang is
/// worse than a wrong frame, which is this file's own rule about
/// guarding an expression where it can leave its domain.
fn stations(run_m: f64, every: f64) -> usize {
    const MOST: f64 = 4096.0;
    (run_m / every).clamp(0.0, MOST) as usize + 2
}

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

/// The LAMP POSTS along one piece of a lit stretch, staggered.
///
/// Walked in metres for the reason the dashes are: a lamp every third
/// PIECE is a lamp every kilometre, and the port's whole lit approach,
/// which is about a kilometre of open road between the town's own band
/// and `road::LIT_NEAR`, came out as one light standing where the camera
/// was. Which SIDE a lamp stands on is its own station's parity, so the
/// stagger is continuous across a piece boundary and across a stretch's.
fn posts(m: &mut Model, a: (DVec3, DVec3), b: (DVec3, DVec3), along: f64, run_m: f64) {
    if !(run_m.is_finite() && along.is_finite() && run_m > 0.0) {
        return;
    }
    let first = (along / LAMP_EVERY).ceil();
    for i in 0..stations(run_m, LAMP_EVERY) {
        let n = first + i as f64;
        if n * LAMP_EVERY >= along + run_m {
            break;
        }
        let t = (n * LAMP_EVERY - along) / run_m;
        let (p, w) = (a.0.lerp(b.0, t), a.1.lerp(b.1, t).normalize_or(a.1));
        let side = if (n as i64).rem_euclid(2) == 0 {
            1.0
        } else {
            -1.0
        };
        let foot = p + w * (side * (HALF + LAMP_OUT));
        let head = foot + DVec3::Z * LAMP_H;
        // The post only DRAWS, which is this file's own rule that
        // anything a body should pass through is trim: a lamp post is
        // not what stops a car.
        //
        // CONCRETE and not PLATE, which is a texture scale rather than a
        // taste: hull plate's panel lines are centimetres apart on a
        // column 0.18 m across, so the first picture of a lit road had a
        // candy striped post in the foreground. Concrete's panels are
        // three metres, so a seven metre standard carries two seams and
        // is otherwise the flat grey a lamp standard is.
        m.trim(
            (foot + head) * 0.5,
            DVec3::new(0.09, 0.09, LAMP_H * 0.5),
            0.0,
            CONCRETE,
        );
        m.lamp(head);
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
