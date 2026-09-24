//! A piece of STREET as a MODEL: a run of carriageway between two
//! pavements, the crossing where two runs meet, and the market square.
//!
//! Split out of `model.rs` when the square took it over this project's
//! own nine hundred lines. A street is a cross section
//! (`town::street`) and this is that cross section as triangles and
//! boxes: the carriageway a quad that stops nothing, the pavement a
//! solid a body steps up onto, and the markings paint.

use super::Model;
use crate::field::{CONCRETE, PAINT, STONE, STREET};
use crate::town::{paved, Piece, BANDS, KERB, LANE, LIFT, WALK};
use glam::{DVec2, DVec3};

/// How far a pavement's slab is sunk INTO the ground, metres. Its
/// underside is then never a plane the levelled terrain can fight with:
/// dual contouring holds a plane to two millimetres and a slab sitting
/// exactly on it would fleck along its whole length.
const BURY: f64 = 0.15;
/// How wide a painted marking is, metres, and how high over the
/// carriageway it is laid. Four millimetres is nothing an eye can see
/// as a step and is a hundred times the two the paving is flat to.
const PAINT_W: f64 = 0.12;
const PAINT_UP: f64 = 0.004;
/// How far the edge line stands in from the kerb, metres.
const EDGE_IN: f64 = 0.18;
/// What share of a piece the centreline's dash takes, so the gap between
/// two dashes is the rest of it.
const DASH: f64 = 0.55;
/// A flat panel laid in a street's own plane: `lo` and `hi` are its
/// corners east and north, `up` how high it stands over the ground.
fn panel(m: &mut Model, lo: DVec2, hi: DVec2, up: f64, material: u8) {
    m.quad(
        DVec3::new(lo.x, lo.y, up),
        DVec3::new(hi.x, lo.y, up),
        DVec3::new(hi.x, hi.y, up),
        DVec3::new(lo.x, hi.y, up),
        material,
    );
}

/// One slab of raised PAVEMENT, its top `KERB` over the carriageway and
/// its underside buried.
///
/// It is a `solid` and the carriageway is not, and the difference is
/// what a body DOES with each. Five centimetres of paving is under
/// anything and a walker stands on the ground through it; twelve is
/// ankle deep, so a pavement a body could not stand on would be a
/// pavement a body stood IN. It is well under the walker's own sixty
/// centimetre step, so he steps up onto it rather than being stopped by
/// it, and `resolve` never sees it at all because its ring of points
/// starts at the step.
pub(super) fn kerb(m: &mut Model, lo: DVec2, hi: DVec2) {
    let top = LIFT + KERB;
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    m.solid(
        DVec3::new(mid.x, mid.y, (top - BURY) * 0.5),
        DVec3::new(half.x, half.y, (top + BURY) * 0.5),
        0.0,
        CONCRETE,
    );
}

/// A street's MARKINGS: one dash of the centreline a piece, and a solid
/// line down each side of the carriageway.
///
/// Paint rather than geometry standing on the road: a marking is a quad
/// four millimetres over the tarmac in the `PAINT` material, which is
/// the street's own set brightened, so it costs no texture, no second
/// draw and no shader of its own.
fn markings(m: &mut Model, long: f64, northerly: bool) {
    let up = LIFT + PAINT_UP;
    let half = long * 0.5;
    let (dash, w) = (half * DASH, PAINT_W * 0.5);
    let edge = LANE - EDGE_IN - w;
    let mut stripe = |a0: f64, a1: f64, c: f64| {
        let (lo, hi) = if northerly {
            (DVec2::new(c - w, a0), DVec2::new(c + w, a1))
        } else {
            (DVec2::new(a0, c - w), DVec2::new(a1, c + w))
        };
        panel(m, lo, hi, up, PAINT);
    };
    stripe(-dash, dash, 0.0);
    stripe(-half, half, edge);
    stripe(-half, half, -edge);
}

/// A RUN of street: two lanes of carriageway, a raised pavement either
/// side of them, and the markings between.
fn run(long: f64, northerly: bool) -> Model {
    let mut m = Model::new();
    let half = long * 0.5;
    // ALONG the run and ACROSS it, turned into the frame's own east and
    // north, so one body of arithmetic lays a street whichever way it
    // lies. Both bounds stay in order, so every panel still winds up.
    let mut band = |a0: f64, c0: f64, a1: f64, c1: f64, road: bool| {
        let (lo, hi) = if northerly {
            (DVec2::new(c0, a0), DVec2::new(c1, a1))
        } else {
            (DVec2::new(a0, c0), DVec2::new(a1, c1))
        };
        if road {
            panel(&mut m, lo, hi, LIFT, STREET);
        } else {
            kerb(&mut m, lo, hi);
        }
    };
    band(-half, -LANE, half, LANE, true);
    band(-half, LANE, half, LANE + WALK, false);
    band(-half, -LANE - WALK, half, -LANE, false);
    markings(&mut m, long, northerly);
    m
}

/// A CROSSING: the square where two runs meet, as three bands each way.
///
/// The middle cell is always carriageway and the four corners are always
/// pavement; each of the four bands between is carriageway when the arm
/// it lies on is there and pavement when it is not. So a crossroads is a
/// plus of tarmac with four kerbed corners, a bend's kerb turns the
/// corner as an L, and a DEAD END closes with a pavement across it
/// rather than stopping mid cell with an open edge.
fn crossing(arms: u8) -> Model {
    let mut m = Model::new();
    for (bx, &(x0, x1)) in BANDS.iter().enumerate() {
        for (bz, &(z0, z1)) in BANDS.iter().enumerate() {
            let (lo, hi) = (DVec2::new(x0, z0), DVec2::new(x1, z1));
            if paved(arms, bx, bz) {
                panel(&mut m, lo, hi, LIFT, STREET);
            } else {
                kerb(&mut m, lo, hi);
            }
        }
    }
    m
}

/// A piece of street: a straight RUN or the CROSSING at the end of one,
/// laid `LIFT` over the levelled ground.
///
/// The carriageway collides with nothing and the pavement does. The
/// site under a town is levelled, so the ground there is a plane and
/// dual contouring holds a plane to two millimetres; five centimetres
/// of paving clears that by twenty five times and is under anything, so
/// a body walks the ground through it. A twelve centimetre kerb is not:
/// it is what makes a pavement read as one, so it is a box a body
/// stands ON, and it is well under the walker's sixty centimetre step,
/// so he steps up rather than being stopped.
pub fn street(piece: &Piece) -> Model {
    if piece.square() {
        square(piece.w, piece.d)
    } else if piece.run() {
        let long = if piece.northerly() { piece.d } else { piece.w };
        run(long, piece.northerly())
    } else {
        crossing(piece.arms)
    }
}

/// A piece of street at a GRADE of detail, the same ladder a building's
/// own bakes are on: the whole cross section at the two nearest, then
/// FLAT, then one SLAB.
///
/// A street is laid in pieces a few metres long, and each is thirty two
/// triangles of kerbs, markings and pavement ends: a town of seven
/// thousand of them is a quarter of a million triangles of paving,
/// which is more than its buildings at their own farthest bake. Past a
/// couple of hundred metres a kerb's twelve centimetres is under a
/// pixel and a dash of paint is under a tenth of one, so what is left
/// is the carriageway and the pavement tops (`flat`), and past a
/// kilometre one quad over the whole piece (`slab`), which is what a
/// street IS from there: a strip of tarmac through the blocks.
///
/// Nothing at a far grade stops a body. What a body is stopped by is
/// built from the nearest grade and only where a body can reach it.
pub fn street_graded(piece: &Piece, grade: usize) -> Model {
    match grade {
        0 | 1 => street(piece),
        2 => flat(piece),
        _ => slab(piece),
    }
}

/// A piece with its kerbs and paint taken off: the carriageway at its
/// own lift and the pavement's tops at theirs, and no sides to either.
fn flat(piece: &Piece) -> Model {
    let mut m = Model::new();
    let top = LIFT + KERB;
    if piece.square() {
        let h = DVec2::new(piece.w, piece.d) * 0.5;
        for (lo, hi) in tiles(-h, h, SQUARE_FAR_TILE) {
            panel(&mut m, lo, hi, top, CONCRETE);
        }
        return m;
    }
    if piece.run() {
        let half = (if piece.northerly() { piece.d } else { piece.w }) * 0.5;
        for &(c0, c1) in &BANDS {
            let (lo, hi) = if piece.northerly() {
                (DVec2::new(c0, -half), DVec2::new(c1, half))
            } else {
                (DVec2::new(-half, c0), DVec2::new(half, c1))
            };
            let road = c0 < 0.0 && c1 > 0.0;
            let (up, material) = if road {
                (LIFT, STREET)
            } else {
                (top, CONCRETE)
            };
            panel(&mut m, lo, hi, up, material);
        }
        return m;
    }
    for (bx, &(x0, x1)) in BANDS.iter().enumerate() {
        for (bz, &(z0, z1)) in BANDS.iter().enumerate() {
            let (up, material) = if paved(piece.arms, bx, bz) {
                (LIFT, STREET)
            } else {
                (top, CONCRETE)
            };
            panel(&mut m, DVec2::new(x0, z0), DVec2::new(x1, z1), up, material);
        }
    }
    m
}

/// A piece as ONE quad over the whole of it, in tarmac, or in paving for
/// the square.
fn slab(piece: &Piece) -> Model {
    let mut m = Model::new();
    let h = DVec2::new(piece.w, piece.d) * 0.5;
    if piece.square() {
        for (lo, hi) in tiles(-h, h, SQUARE_FAR_TILE) {
            panel(&mut m, lo, hi, LIFT + KERB, CONCRETE);
        }
    } else {
        panel(&mut m, -h, h, LIFT, STREET);
    }
    m
}

/// How wide a square's paving is laid in one piece, metres, and how wide
/// once its kerbs are not worth drawing.
///
/// A square is one PIECE, up to 137 m across in a city, and on graded
/// ground one piece is one plane: laid whole it stood 0.74 m off the
/// ground the town is graded to at its worst. In tiles every tile corner
/// is laid on the ground at its own point, and between two corners the
/// ground bends by the grade's own curvature, `2 GRADE / STEP`, over at
/// most a tile: two centimetres at eight metres.
const SQUARE_TILE: f64 = 8.0;
const SQUARE_FAR_TILE: f64 = 16.0;

/// A rectangle cut into tiles no wider than `most` either way.
fn tiles(lo: DVec2, hi: DVec2, most: f64) -> Vec<(DVec2, DVec2)> {
    let size = hi - lo;
    let (nx, ny) = (
        (size.x / most).ceil().max(1.0) as usize,
        (size.y / most).ceil().max(1.0) as usize,
    );
    let step = DVec2::new(size.x / nx as f64, size.y / ny as f64);
    let mut out = Vec::with_capacity(nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let a = lo + step * DVec2::new(i as f64, j as f64);
            out.push((a, a + step));
        }
    }
    out
}

/// How wide the plinth in the middle of a square is and how high it
/// stands, metres, and how far in from the square's own corners its
/// four lamps stand.
const PLINTH: f64 = 3.0;
const PLINTH_H: f64 = 1.2;
const SQUARE_LAMP_H: f64 = 5.0;

/// The market SQUARE: one raised slab of paving at kerb height over the
/// whole of it, a stone plinth in the middle and a lamp standing in
/// from each corner. A body steps up onto it as onto a pavement, which
/// is what a square is, and nothing on it stops a body but the plinth.
fn square(w: f64, d: f64) -> Model {
    let mut m = Model::new();
    let (hw, hd) = (w * 0.5, d * 0.5);
    for (lo, hi) in tiles(DVec2::new(-hw, -hd), DVec2::new(hw, hd), SQUARE_TILE) {
        kerb(&mut m, lo, hi);
    }
    let top = LIFT + KERB;
    m.solid(
        DVec3::new(0.0, 0.0, top + PLINTH_H * 0.5),
        DVec3::new(PLINTH * 0.5, PLINTH * 0.5, PLINTH_H * 0.5),
        0.0,
        STONE,
    );
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let foot = DVec3::new(sx * (hw - LANE), sy * (hd - LANE), top);
        let head = foot + DVec3::Z * SQUARE_LAMP_H;
        m.trim(
            (foot + head) * 0.5,
            DVec3::new(0.09, 0.09, SQUARE_LAMP_H * 0.5),
            0.0,
            CONCRETE,
        );
        m.lamp(head);
    }
    m
}
