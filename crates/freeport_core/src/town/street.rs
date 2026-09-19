//! A town's STREETS: what one is made of ACROSS, and where they run.
//!
//! This is mining-mike's lesson (`scripts/meridian/city_road.gd` in
//! godot-sandbox) carried onto a grid. What it replaced there was one
//! MultiMesh of flat boxes, and its own note on why says it best: a box
//! has no CROSS SECTION, so the road had no kerb, no pavement and no
//! markings, its corners met at hard mitres, and a dead end simply
//! stopped mid cell with an open edge. This street was a single quad
//! four metres wide, which is all of those and too narrow for two cars
//! besides.
//!
//! So a street is three things rather than one: the two lanes of
//! carriageway, the raised pavement either side of them, and the
//! CROSSING where two of them meet, which is a piece of its own that
//! owns its whole square. What decides all three is the same handful of
//! numbers below, because a cross section is one number said several
//! ways and never several numbers that have to agree.
//!
//! Coordinates are the town's, x east and z north in metres.

use super::fronts;

/// Which ARMS a crossing has: the four ways a street can leave it.
///
/// They are the same four bits `fronts` uses, because they are the same
/// four directions and two tables of them would be two tables to get
/// wrong.
pub mod arm {
    pub const WEST: u8 = 1;
    pub const EAST: u8 = 2;
    pub const SOUTH: u8 = 4;
    pub const NORTH: u8 = 8;
}

/// The three BANDS a street is cut into ACROSS it: the pavement, the
/// two lanes of carriageway, the pavement. A crossing is those same
/// three bands each way, nine cells of it, and `paved` says which of
/// them are carriageway.
pub const BANDS: [(f64, f64); 3] = [(-STREET * 0.5, -LANE), (-LANE, LANE), (LANE, STREET * 0.5)];

/// Which band a coordinate across a street falls in.
pub fn band(v: f64) -> usize {
    if v < -LANE {
        0
    } else if v > LANE {
        2
    } else {
        1
    }
}

/// Whether the cell of a crossing at band `bx` east and `bz` north is
/// CARRIAGEWAY rather than pavement.
///
/// The middle cell always is and the four corners never are; each of
/// the four bands between is carriageway exactly when the arm it lies
/// on is there. So a crossroads is a plus of tarmac with four kerbed
/// corners, a bend's kerb turns the corner as an L, and a dead end
/// closes with a pavement across it.
///
/// It is ONE function because two things ask it: the mesh that draws a
/// crossing, and the traffic, which has to know where a pedestrian
/// steps down off the kerb. A second copy of these nine cells is a
/// person walking a hand over the tarmac at one junction in a hundred,
/// which nothing would say.
pub fn paved(arms: u8, bx: usize, bz: usize) -> bool {
    match (bx, bz) {
        (1, 1) => true,
        (1, 2) => arms & arm::NORTH != 0,
        (1, 0) => arms & arm::SOUTH != 0,
        (2, 1) => arms & arm::EAST != 0,
        (0, 1) => arms & arm::WEST != 0,
        _ => false,
    }
}

/// A piece of street: its middle, its size east and north, in town metres.
///
/// `arms` is nought for a RUN, which is a straight length of street
/// between two crossings, and is the arms it has for a CROSSING, which
/// is the square where two runs meet. The two are different things and
/// the same struct, because a crossing is a run's own cross section
/// turned through a right angle over the top of itself: what decides
/// where its kerbs stand is which arms it has, and a run has none.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub arms: u8,
}

impl Piece {
    /// Whether this is a straight RUN rather than a crossing.
    pub fn run(&self) -> bool {
        self.arms == 0
    }

    /// Whether a run points north and south rather than east and west.
    /// A run is a piece of a street and a street is `STREET` across, so
    /// which of its two sizes IS the street says which way it lies.
    pub fn northerly(&self) -> bool {
        self.w > self.d
    }
}

/// A street's own CROSS SECTION, which is what a street is: two lanes of
/// carriageway with a raised pavement either side of them.
///
/// This is mining-mike's lesson (`scripts/meridian/city_road.gd` in
/// godot-sandbox) carried onto a grid. What it replaced there was one
/// MultiMesh of flat boxes, and its own note on why says it best: "a box
/// has no cross section, so the road had no kerb, no shoulder and no
/// markings, its corners met at hard mitres, and a dead end simply
/// stopped mid cell with an open edge." This street was a single quad
/// four metres wide, which is all three of those things and too narrow
/// for two cars besides.
///
/// One LANE is 2.75 m and a car is 1.6 across, so two pass with better
/// than a metre between them; one WALK is 1.5 m and a person is 0.45, so
/// two pass on the pavement too. The street is the sum of them, and the
/// PITCH is that plus the block, because those are not three numbers
/// that have to agree, they are one number said three ways.
pub const LANE: f64 = 2.75;
pub const WALK: f64 = 1.5;
pub const BLOCK: f64 = 10.0;
pub const STREET: f64 = 2.0 * (LANE + WALK);
pub const PITCH: f64 = BLOCK + STREET;

/// How high the pavement stands over the carriageway, metres. Well under
/// the walker's own 0.6 m step, so a kerb is stepped up rather than
/// climbed, and it is the whole of what makes a pavement read as one
/// rather than as a stripe of lighter paving.
pub const KERB: f64 = 0.12;

/// How far a street's surface stands over the ground it is laid on,
/// metres. The site under a town is LEVELLED, so the ground there is a
/// plane and dual contouring holds a plane to two millimetres (the
/// audit's own number); five centimetres clears that by twenty five
/// times and is under any step, so the paving needs no collider of its
/// own and a walker never sinks into a kerb.
pub const LIFT: f64 = 0.05;

/// A street's RUN between two crossings is laid in pieces about this
/// long, each on its own patch of the sphere.
pub const PIECE: f64 = 3.5;
/// The one side a suburban block fronts: the side its road home is on.
///
/// A suburb block used to front all four sides like a downtown one, and
/// a lone house with nothing built beside it then stood in a square ring
/// of its own tarmac: a moat.
///
/// What replaced that fronted the side FACING the middle of town, and
/// that is the wrong side, because the street on it RUNS ACROSS the way
/// home: a house far out along east fronted west, which paves a NORTH
/// SOUTH street beside it, and nothing on that street leads west. Every
/// suburban house came out with an isolated rectangle of tarmac at its
/// door, which is what the owner's picture showed. The side to front is
/// the one whose street runs ALONG the axis the middle of town is down,
/// and `home_run` is then what carries that street all the way in.
pub(super) fn faces(i: i64, j: i64) -> u8 {
    if i.abs() >= j.abs() {
        // Far out along east: the road home runs east and west, which is
        // the street on this block's own south side.
        fronts::SOUTH
    } else {
        fronts::WEST
    }
}

/// Every block whose frontage has to be paved so that the block at
/// `(i, j)` can be DRIVEN TO from the middle of town, as an L: out along
/// the axis it stands furthest down, then in along the other.
///
/// A road that serves one house and joins nothing is not a road. The
/// union of these over every built block is the collector network a
/// suburb hangs off: the ribs are the rows and columns somebody built
/// on and the two trunks are the axes through the middle, and in the
/// dense core they merge into the grid that was there anyway.
pub(super) fn home_run(i: i64, j: i64, mut mark: impl FnMut(i64, i64, u8)) {
    let between = |a: i64, b: i64| a.min(b)..=a.max(b);
    if i.abs() >= j.abs() {
        for k in between(0, i) {
            mark(k, j, fronts::SOUTH);
        }
        for m in between(0, j) {
            mark(0, m, fronts::WEST);
        }
    } else {
        for m in between(0, j) {
            mark(i, m, fronts::WEST);
        }
        for k in between(0, i) {
            mark(k, 0, fronts::SOUTH);
        }
    }
}

/// The streets of a town: a piece wherever a street runs past a block
/// somebody built on, and nowhere else.
///
/// Laid over the whole disc instead, which is what this did, a town's
/// paving was a circle whatever shape the town itself came out, so the
/// outline the lobes cut was hidden under a perfectly round grid of
/// tarmac. A street that serves nothing is not a street.
pub(super) fn streets_of(n: i64, built: &[u8]) -> Vec<Piece> {
    let wide = (2 * n + 1) as usize;
    let at = |i: i64, j: i64, side: u8| {
        (-n..=n).contains(&i)
            && (-n..=n).contains(&j)
            && built[(i + n) as usize * wide + (j + n) as usize] & side != 0
    };
    // Along the block's own edge: the line between block i - 1 and i.
    let line = |i: i64| i as f64 * PITCH - BLOCK / 2.0 - STREET / 2.0;
    // A run spans the BLOCK it serves and stops at the crossing squares
    // either end of it, which is what leaves a crossing somewhere to be.
    // Cut into whole pieces of about `PIECE`, each on its own patch of
    // the sphere, so the run exactly fills the gap and nothing overlaps.
    let steps = (BLOCK / PIECE).ceil().max(1.0);
    let cut = BLOCK / steps;
    let steps = steps as i64;
    let mut pieces = Vec::new();
    // Every street between block i - 1 and block i, as far as the blocks
    // either side of it are built on: the paving is continuous along a
    // run of built blocks and stops with them.
    let northerly = |i: i64, j: i64| at(i - 1, j, fronts::EAST) || at(i, j, fronts::WEST);
    let easterly = |i: i64, j: i64| at(i, j - 1, fronts::NORTH) || at(i, j, fronts::SOUTH);
    for i in -n..=n + 1 {
        for j in -n..=n {
            if !northerly(i, j) {
                continue;
            }
            for k in 0..steps {
                let mid = j as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * cut;
                pieces.push(Piece {
                    x: line(i),
                    z: mid,
                    w: STREET,
                    d: cut,
                    arms: 0,
                });
            }
        }
    }
    for j in -n..=n + 1 {
        for i in -n..=n {
            if !easterly(i, j) {
                continue;
            }
            for k in 0..steps {
                let mid = i as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * cut;
                pieces.push(Piece {
                    x: mid,
                    z: line(j),
                    w: cut,
                    d: STREET,
                    arms: 0,
                });
            }
        }
    }
    // And a CROSSING wherever two of those streets could meet, which is
    // every corner of every block. It is the square the runs stop short
    // of, and it carries whichever arms actually leave it: with all four
    // it is a crossroads, with two at a right angle it is a bend whose
    // kerb turns the corner, and with one it is a dead end that closes.
    for i in -n..=n + 1 {
        for j in -n..=n + 1 {
            let mut arms = 0u8;
            if northerly(i, j) {
                arms |= arm::NORTH;
            }
            if northerly(i, j - 1) {
                arms |= arm::SOUTH;
            }
            if easterly(i, j) {
                arms |= arm::EAST;
            }
            if easterly(i - 1, j) {
                arms |= arm::WEST;
            }
            if arms == 0 {
                continue;
            }
            pieces.push(Piece {
                x: line(i),
                z: line(j),
                w: STREET,
                d: STREET,
                arms,
            });
        }
    }
    pieces
}
