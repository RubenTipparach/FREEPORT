//! A building's ROOF: a flat slab with a parapet, a gable or a barrel
//! vault, laid over walls `h` metres high on a `w` by `d` footprint.

use super::*;

/// The parapet round a flat roof: how high it stands over the slab and how
/// thick it is, metres.
const PARAPET: f64 = 0.7;
const PARAPET_T: f64 = 0.22;
/// How far a pitched roof's eaves overhang the walls under them, metres.
const EAVE: f64 = 0.3;
/// How many pieces a barrel vault's arc is cut into.
const ARCH: usize = 9;

/// A flat roof: a slab and a parapet round it.
pub(super) fn flat_roof(m: &mut Model, w: f64, d: f64, h: f64) {
    let (hw, hd) = (w * 0.5, d * 0.5);
    m.solid(
        DVec3::new(0.0, 0.0, h + SLAB * 0.5),
        DVec3::new(hw, hd, SLAB * 0.5),
        0.0,
        CONCRETE,
    );
    let z = h + SLAB + PARAPET * 0.5;
    let t = PARAPET_T * 0.5;
    for side in [-1.0, 1.0] {
        m.trim(
            DVec3::new(0.0, side * (hd - t), z),
            DVec3::new(hw, t, PARAPET * 0.5),
            0.0,
            PLATE,
        );
        m.trim(
            DVec3::new(side * (hw - t), 0.0, z),
            DVec3::new(t, hd - PARAPET_T, PARAPET * 0.5),
            0.0,
            PLATE,
        );
    }
}

/// A gable: two pitched faces to a ridge along the lot's east axis, and a
/// face closing each end.
///
/// The eaves overhang by `EAVE` front and back and the gable ends not at
/// all: terraced neighbours share a height and a pitch, so a roof past its
/// own lot lay in the next one's plane, a strip of z-fighting at every
/// party wall.
pub(super) fn gable(m: &mut Model, w: f64, d: f64, h: f64, skin: u8) {
    let (hw, hd) = (w * 0.5, d * 0.5 + EAVE);
    let ridge = h + d * 0.35;
    for side in [-1.0, 1.0] {
        let eave = DVec3::new(0.0, side * hd, h);
        let a = eave - DVec3::X * hw;
        let b = eave + DVec3::X * hw;
        let c = DVec3::new(hw, 0.0, ridge);
        let e = DVec3::new(-hw, 0.0, ridge);
        if side < 0.0 {
            m.quad(a, b, c, e, skin);
        } else {
            m.quad(b, a, e, c, skin);
        }
    }
    // The end: flush with the wall over the wall's own width, and the two
    // wedges under the eaves `INSET` inside that plane, because past the
    // wall they stand over the NEXT lot, whose wall on that line is in the
    // same plane (`no_two_faces_of_a_town_fight_for_one_plane`).
    let (wall, at_wall) = (d * 0.5, h + (ridge - h) * EAVE / hd);
    let yz = |y: f64, z: f64| DVec2::new(y, z);
    let over = [
        yz(-wall, h),
        yz(wall, h),
        yz(wall, at_wall),
        yz(0.0, ridge),
        yz(-wall, at_wall),
    ];
    for side in [-1.0f64, 1.0] {
        let end = |x: f64, p: DVec2| DVec3::new(x, p.x, p.y);
        let mut tri = |x: f64, a: DVec2, b: DVec2, c: DVec2| {
            // Wound to face out of the side it closes.
            let ccw = (b - a).perp_dot(c - a) * side > 0.0;
            let (b, c) = if ccw { (b, c) } else { (c, b) };
            m.tri(end(x, a), end(x, b), end(x, c), skin);
        };
        for k in 1..over.len() - 1 {
            tri(side * hw, over[0], over[k], over[k + 1]);
        }
        let x = side * (hw - INSET);
        for s in [-1.0, 1.0] {
            tri(x, yz(s * wall, h), yz(s * hd, h), yz(s * wall, at_wall));
        }
    }
}

/// A barrel vault: an arc of quads over the lot's north axis, with the
/// ends left open, which is what a hangar looks like.
pub(super) fn vault(m: &mut Model, w: f64, d: f64, h: f64) {
    let r = w * 0.5;
    let hd = d * 0.5;
    let at = |k: usize| {
        let a = std::f64::consts::PI * k as f64 / ARCH as f64;
        DVec3::new(-r * a.cos(), 0.0, h + r * a.sin() * 0.55)
    };
    for k in 0..ARCH {
        let (p, q) = (at(k), at(k + 1));
        m.quad(
            p - DVec3::Y * hd,
            q - DVec3::Y * hd,
            q + DVec3::Y * hd,
            p + DVec3::Y * hd,
            PLATE,
        );
    }
}
