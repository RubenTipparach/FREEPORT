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

use crate::field::{ARC_SKIRT, CONCRETE, PAINT, STREET, TERRAIN};
use crate::model::Model;
use crate::town::{Frame, LANE};
use glam::DVec3;

/// One run of a road, as the four things the ribbon reads about it: the
/// refined centreline, the level the corridor was cut to under each
/// point, which points are outside every town's own levelling, and which
/// are near enough a settlement to carry a lamp.
///
/// One struct rather than four slices, because they are indexed together
/// at every line of this file and a fifth would be the fifth place to get
/// an index wrong; it is the app's own `Route` said in the core's terms.
#[derive(Clone, Copy)]
pub struct Course<'a> {
    pub line: &'a [DVec3],
    pub run: &'a [f64],
    /// Where TARMAC is laid, which includes the slip a highway runs into
    /// a town on.
    pub open: &'a [bool],
    /// Where this road's own CORRIDOR was cut, which the slip is not:
    /// a slip stands on ground the TOWN levelled, so the road has no
    /// embankment to draw there and a 32 m apron over somebody's streets
    /// is not an embankment. One flag doing both jobs is the thing this
    /// pair replaces, and the comment in the app's own splice already
    /// named it as the thing to watch.
    pub graded: &'a [bool],
    /// Which points are near enough a settlement to carry a lamp.
    pub lit: &'a [bool],
    /// The GAS STATIONS on the ROAD, each by the piece of the whole road
    /// it stands on (`road::station::plan`), and which of those pieces
    /// this course starts at, so a stretch finds its own among them.
    pub pumps: &'a [super::station::Station],
    pub first: usize,
}

impl Course<'_> {
    /// Whether every lane of it is the same length and there is a piece
    /// in it at all.
    fn sound(&self) -> bool {
        self.line.len() >= 2
            && self.run.len() == self.line.len()
            && self.open.len() == self.line.len()
    }

    /// Whether the corridor was cut at a point, which is false past the
    /// end of a lane an older caller did not fill.
    fn cut(&self, k: usize) -> bool {
        self.graded.get(k).copied().unwrap_or(false)
    }
}

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
pub const SHOULDER: f64 = 0.7;
pub const BURIED: f64 = -0.15;

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
/// carriageway, a dashed centreline, a solid line down each edge, and
/// the MOUND the whole of it stands on.
///
/// `course.line` is the refined centreline (`road::centreline`) and
/// `course.run` the ground under it (`road::survey`), which is the level
/// the corridor was cut to, so the tarmac sits `LIFT` over a plane
/// rather than over whatever the hill used to be.
///
/// `ground` is the field's own surface at a direction, metres over the
/// mean radius, and it is what the mound is drawn from: the batter this
/// road stands on is the corridor's own skirt, so handing the ribbon the
/// same function the mesher and the walker read is what makes the drawn
/// mound and the collided one one surface (`mound`).
pub fn stretch(
    frame: &Frame,
    course: Course<'_>,
    radius: f64,
    ground: &dyn Fn(DVec3) -> f64,
) -> Model {
    let mut m = Model::new();
    if !course.sound() {
        return m;
    }
    let (open, lit) = (course.open, course.lit);
    let station = stations(frame, course, radius);
    // The MOUND first, so the tarmac's own triangles are laid over it in
    // the same mesh and a reader of the model meets the ground before
    // what stands on it.
    mound(&mut m, frame, course, &station, radius, ground);
    let mut along = 0.0;
    for k in 0..station.len() - 1 {
        let run_m = (station[k + 1].0 - station[k].0).length();
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
        let ((a, u), (b, v)) = (station[k], station[k + 1]);
        // The dashes keep the phase of the WHOLE piece, so a road whose
        // last piece starts part way along does not restart its
        // markings at the junction.
        let (from, laid) = (along, run_m);
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
    // And the GAS STATIONS, each one a forecourt set down on the verge
    // at the middle of its own piece, turned to the road.
    for pump in course.pumps {
        let Some(k) = pump.piece.checked_sub(course.first) else {
            continue;
        };
        let Some((&(a, u), &(b, v))) = station.get(k).zip(station.get(k + 1)) else {
            continue;
        };
        if !(open[k] && open[k + 1]) {
            continue;
        }
        let across = (u + v).normalize_or(u) * pump.side;
        let at = (a + b) * 0.5 + across * super::station::setback();
        let seed = super::station::seed_of(pump.piece);
        m.place(
            &super::station::forecourt(seed),
            at,
            across.y.atan2(across.x),
        );
    }
    m
}

/// Every centreline point in the stretch's own frame, with the unit
/// vector ACROSS the road there.
///
/// Worked out once rather than by a closure called from four places: the
/// across at a station is a difference of its two neighbours, so a
/// closure that recomputed it cost three placements per call and could
/// not be handed to `mound` without handing over the whole of the
/// arithmetic with it.
fn stations(frame: &Frame, course: Course<'_>, radius: f64) -> Vec<(DVec3, DVec3)> {
    let (line, run) = (course.line, course.run);
    let at = |k: usize| frame.local(line[k] * (radius + run[k] + LIFT));
    (0..line.len())
        .map(|k| {
            let ahead = (k + 1).min(line.len() - 1);
            let back = k.saturating_sub(1);
            let along = at(ahead) - at(back);
            // The frame's up is its own z, so across is what is square
            // to the road in the frame's own tangent plane.
            (
                at(k),
                DVec3::new(along.y, -along.x, 0.0).normalize_or(DVec3::X),
            )
        })
        .collect()
}

/// Where the MOUND's own bands stand, metres from the centreline: the
/// shoulder's outer edge, the flat verge out to the corridor's own
/// levelled width, and then the batter down its skirt.
///
/// Three bands over the flat, which the field holds to a plane and which
/// needs no more; four over the skirt, which is a smoothstep eleven
/// metres wide, so a chord over 2.75 m of it stands a few centimetres
/// off the blend it is drawn from, which is under the sink that hides
/// it.
fn bands() -> [f64; 8] {
    let flat = HALF + SHOULDER;
    let verge = |t: f64| flat + (super::CORRIDOR - flat) * t;
    let batter = |t: f64| super::CORRIDOR + ARC_SKIRT * t;
    [
        verge(0.0),
        verge(1.0 / 3.0),
        verge(2.0 / 3.0),
        batter(0.0),
        batter(0.25),
        batter(0.5),
        batter(0.75),
        batter(1.0),
    ]
}

/// The MOUND a road stands on: the verge out to the corridor's own
/// levelled width and the batter down its skirt, both sides, drawn from
/// the field's own surface.
///
/// This is the owner's ask and it is the answer to a defect this project
/// had already measured from the air. A corridor is `2 * CORRIDOR` of
/// flat and the rings put a cell of about a sixty fourth of its own
/// distance under the eye, so past a couple of kilometres the mesher has
/// no sample inside the corridor at all and draws the hill that was
/// there before the road; the tarmac is laid to `roads::REACH`, nine
/// kilometres, whatever the terrain does. Between those two ranges the
/// road is a ribbon hanging over a hill that does not know about it, and
/// the hill wins wherever it stands higher. **So the road CARRIES its
/// own ground.**
///
/// Every point of it is `ground(dir)`, which is the same
/// `Planet::surface` the mesher contours and the walker and the car
/// collide against, so the mound that is DRAWN and the mound that is
/// STOOD ON are one surface by construction rather than two that have to
/// agree. That is this project's own rule about a wall being one box
/// that is drawn and collided, arriving at the one thing here that is
/// not a box.
///
/// It is SUNK `BURIED` under that surface, which is the pavement slab's
/// own trick: wherever the terrain really is drawn at this detail the
/// terrain wins and the mound is inside the hill, and wherever it is not
/// the mound is the ground.
///
/// It is drawn where the corridor was CUT (`course.graded`) and not
/// merely where tarmac is laid: a slip runs over a town's own plateau,
/// which the town levelled and this road did not, and a 32 m apron
/// crossing somebody's streets is not an embankment.
fn mound(
    m: &mut Model,
    frame: &Frame,
    course: Course<'_>,
    station: &[(DVec3, DVec3)],
    radius: f64,
    ground: &dyn Fn(DVec3) -> f64,
) {
    let edges = bands();
    // One column of the cross section at a station: where each band's
    // edge stands, in the frame, on the field's own surface.
    //
    // Every column is worked out ONCE and kept, because a station is
    // the far end of one piece and the near end of the next: asked per
    // piece, a stretch sampled the field 2,080 times where 1,040 will
    // do, and a sample here is a whole `Planet::surface`, which is five
    // terms of eighteen octaves and the body's own site index.
    let column = |k: usize, side: f64| -> [DVec3; 8] {
        let (here, across) = station[k];
        std::array::from_fn(|j| {
            let dir = frame
                .world(here + across * (side * edges[j]))
                .normalize_or(DVec3::Y);
            frame.local(dir * (radius + ground(dir) + BURIED))
        })
    };
    let wanted = |k: usize| k + 1 < station.len() && course.cut(k) && course.cut(k + 1);
    let cut: Vec<[[DVec3; 8]; 2]> = (0..station.len())
        .map(|k| {
            // A station no piece either side of it is cut at is a
            // station nothing is drawn from, and its column is the
            // sampling this saves.
            if wanted(k) || (k > 0 && wanted(k - 1)) {
                [column(k, -1.0), column(k, 1.0)]
            } else {
                [[DVec3::ZERO; 8]; 2]
            }
        })
        .collect();
    for k in 0..station.len() - 1 {
        if !wanted(k) {
            continue;
        }
        for (s, side) in [-1.0f64, 1.0].iter().enumerate() {
            let (p, q) = (&cut[k][s], &cut[k + 1][s]);
            for j in 0..edges.len() - 1 {
                // Wound so the face is up whichever side it is on.
                if *side > 0.0 {
                    m.quad(p[j], p[j + 1], q[j + 1], q[j], TERRAIN);
                } else {
                    m.quad(q[j], q[j + 1], p[j + 1], p[j], TERRAIN);
                }
            }
        }
    }
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
    for i in 0..marks(run_m, cycle) {
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

/// How many MARKS of a given spacing a piece of road is walked at, and
/// a CAP on it.
///
/// A piece is `road::PIECE` and the closest spacing anything here is
/// laid at is a dash's nine metres, so forty is the real count. Four
/// thousand is a piece a hundred times longer than one can be, and it is
/// there because a loop stepping a fixed distance over a length it was
/// handed is a loop a garbage line hangs the mesher with: a hang is
/// worse than a wrong frame, which is this file's own rule about
/// guarding an expression where it can leave its domain.
fn marks(run_m: f64, every: f64) -> usize {
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
        lean: glam::DVec2::ZERO,
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
    for i in 0..marks(run_m, LAMP_EVERY) {
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
