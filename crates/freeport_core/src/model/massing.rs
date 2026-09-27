//! A building as a SOLID BLOCK: what a city is drawn as past the
//! distance its detail is worth.
//!
//! The owner's ask is whole cities of thousands of buildings in one
//! picture, and a building's detail is what stops that: the baked
//! library's nearest bake is five to eleven thousand triangles, with
//! real window holes and the room behind them, and its farthest is still
//! a hundred and more. From a kilometre off a building is a few pixels,
//! and what those pixels say is its footprint, its height and what it is
//! built of. That is a box of its own skin to its own eaves, a gable
//! where a house has one and a drum where a tower is round: ten to
//! twenty two triangles whatever the building was.
//!
//! It is read OFF THE MODEL rather than written beside it, so a block
//! cannot drift from the building it stands for: the footprint and the
//! eaves are the model's own SOLIDS, which are the walls a body is
//! stopped by, and the top is its own mesh, which is its roof and its
//! parapet. A variant re-baked taller comes back taller at every range
//! with nothing here edited.

use super::{Kind, Model};
use crate::field::{CONCRETE, PLATE};
use glam::DVec3;

/// How far a block is sunk under its own lot, metres. The ground under a
/// town is levelled, so it is a plane to two millimetres, but a coarse
/// terrain chunk far off is not, and a block standing exactly on the
/// level would show a sliver of sky under its downhill edge wherever the
/// drawn ground stands a little lower.
const SINK: f64 = 0.4;
/// How many faces a round tower's block is drawn with. Eight reads as
/// round at the few pixels a block is drawn at and is a third of the
/// detail model's twelve.
const FACETS: usize = 8;
/// How far a roof has to stand over the eaves to be drawn as a GABLE
/// rather than folded into the box, metres: a parapet is under this and
/// a pitched roof is well over it.
const PITCHED: f64 = 1.0;

/// The block a model stands for: its own footprint to its own eaves in
/// `skin`, capped the way its roof is.
pub fn massing(of: &Model, kind: Kind, skin: u8) -> Model {
    let mut m = Model::new();
    let Some((lo, hi, eave)) = extent(of) else {
        return m;
    };
    let top = of.high().max(eave);
    let pitched = kind == Kind::House && top - eave > PITCHED;
    match kind {
        Kind::Tower => drum(&mut m, (hi.x - lo.x).min(hi.y - lo.y) * 0.5, top, skin),
        _ if pitched => {
            walls(&mut m, lo, hi, eave, skin);
            gable(&mut m, lo, hi, eave, top, skin);
        }
        _ => {
            walls(&mut m, lo, hi, top, skin);
            let lid = if kind == Kind::Hangar {
                PLATE
            } else {
                CONCRETE
            };
            m.quad(
                DVec3::new(lo.x, lo.y, top),
                DVec3::new(hi.x, lo.y, top),
                DVec3::new(hi.x, hi.y, top),
                DVec3::new(lo.x, hi.y, top),
                lid,
            );
        }
    }
    m
}

/// A model's footprint and its eaves, off its SOLIDS: the walls a body is
/// stopped by, which is the building and not its lamp over the door or
/// the overhang of its roof. `None` for a model with nothing solid in it.
fn extent(of: &Model) -> Option<(DVec3, DVec3, f64)> {
    let mut lo = DVec3::INFINITY;
    let mut hi = DVec3::NEG_INFINITY;
    for s in &of.solids {
        let a = s.axes();
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let c = s.centre
                        + a[0] * (sx * s.half.x)
                        + a[1] * (sy * s.half.y)
                        + a[2] * (sz * s.half.z);
                    lo = lo.min(c);
                    hi = hi.max(c);
                }
            }
        }
    }
    (lo.is_finite() && hi.is_finite() && hi.x > lo.x && hi.y > lo.y).then_some((lo, hi, hi.z))
}

/// Four walls from under the lot to `h`, each a quad wound out of the
/// block.
fn walls(m: &mut Model, lo: DVec3, hi: DVec3, h: f64, skin: u8) {
    let z0 = -SINK;
    let c = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    m.quad(
        c(lo.x, lo.y, z0),
        c(hi.x, lo.y, z0),
        c(hi.x, lo.y, h),
        c(lo.x, lo.y, h),
        skin,
    );
    m.quad(
        c(hi.x, lo.y, z0),
        c(hi.x, hi.y, z0),
        c(hi.x, hi.y, h),
        c(hi.x, lo.y, h),
        skin,
    );
    m.quad(
        c(hi.x, hi.y, z0),
        c(lo.x, hi.y, z0),
        c(lo.x, hi.y, h),
        c(hi.x, hi.y, h),
        skin,
    );
    m.quad(
        c(lo.x, hi.y, z0),
        c(lo.x, lo.y, z0),
        c(lo.x, lo.y, h),
        c(lo.x, hi.y, h),
        skin,
    );
}

/// A gable over the eaves: two pitched faces to a ridge along the lot's
/// east axis and a triangle closing each end, which is `model::gable`'s
/// own shape at the block's footprint.
fn gable(m: &mut Model, lo: DVec3, hi: DVec3, eave: f64, ridge: f64, skin: u8) {
    let mid = (lo.y + hi.y) * 0.5;
    let c = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    m.quad(
        c(lo.x, lo.y, eave),
        c(hi.x, lo.y, eave),
        c(hi.x, mid, ridge),
        c(lo.x, mid, ridge),
        skin,
    );
    m.quad(
        c(hi.x, hi.y, eave),
        c(lo.x, hi.y, eave),
        c(lo.x, mid, ridge),
        c(hi.x, mid, ridge),
        skin,
    );
    m.tri(
        c(lo.x, lo.y, eave),
        c(lo.x, mid, ridge),
        c(lo.x, hi.y, eave),
        skin,
    );
    m.tri(
        c(hi.x, lo.y, eave),
        c(hi.x, hi.y, eave),
        c(hi.x, mid, ridge),
        skin,
    );
}

/// A round tower as a drum of `FACETS` faces about its own middle, with a
/// flat concrete lid.
fn drum(m: &mut Model, r: f64, h: f64, skin: u8) {
    let at = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * k as f64 / FACETS as f64;
        DVec3::new(r * a.cos(), r * a.sin(), z)
    };
    for k in 0..FACETS {
        m.quad(at(k, -SINK), at(k + 1, -SINK), at(k + 1, h), at(k, h), skin);
    }
    let middle = DVec3::new(0.0, 0.0, h);
    for k in 0..FACETS {
        m.tri(middle, at(k, h), at(k + 1, h), CONCRETE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::building;

    /// Every triangle of a block faces OUT of it, which is what a back
    /// face cull keeps: a block wound inside out is the mockup's black
    /// kit a second time.
    fn faces_out(m: &Model, middle: DVec3) -> bool {
        m.mesh.indices.chunks(3).all(|t| {
            let p: Vec<DVec3> = t
                .iter()
                .map(|&i| DVec3::from(m.mesh.positions[i as usize].map(f64::from)))
                .collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let c = (p[0] + p[1] + p[2]) / 3.0;
            n.dot(c - middle) > 0.0
        })
    }

    #[test]
    fn a_block_is_the_buildings_own_footprint_height_and_skin_and_faces_out() {
        for kind in Kind::all() {
            let (lo, hi) = kind.storeys();
            for n in [lo, hi] {
                let full = building(kind, 10.0, 10.0, n, 5);
                let skin = kind.skin(5);
                let block = massing(&full, kind, skin);
                let (flo, fhi, eave) = extent(&full).expect("a building has walls");
                let top = full.high();
                let (mut blo, mut bhi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
                for p in &block.mesh.positions {
                    let p = DVec3::from(p.map(f64::from));
                    blo = blo.min(p);
                    bhi = bhi.max(p);
                }
                assert!(
                    (bhi.z - top).abs() < 1e-3,
                    "{} {n}: the block tops out at {:.2} where the building does at {top:.2}",
                    kind.name(),
                    bhi.z
                );
                assert!(eave <= top + 1e-6);
                assert!(
                    blo.x >= flo.x - 1e-3 && bhi.x <= fhi.x + 1e-3,
                    "{}",
                    kind.name()
                );
                assert!(
                    blo.y >= flo.y - 1e-3 && bhi.y <= fhi.y + 1e-3,
                    "{}",
                    kind.name()
                );
                assert!(
                    blo.z < 0.0,
                    "{} stands on the ground and not in it",
                    kind.name()
                );
                assert!(
                    block.mesh.materials.contains(&skin),
                    "{} is not in its own skin",
                    kind.name()
                );
                let middle = DVec3::new((flo.x + fhi.x) * 0.5, (flo.y + fhi.y) * 0.5, top * 0.4);
                assert!(
                    faces_out(&block, middle),
                    "{} is wound inside out",
                    kind.name()
                );
                assert!(
                    block.mesh.triangles() <= 24,
                    "{} is {} triangles",
                    kind.name(),
                    block.mesh.triangles()
                );
                assert!(block.solids.is_empty() && block.lamps.is_empty());
            }
        }
    }

    #[test]
    fn a_model_with_nothing_solid_in_it_has_no_block() {
        assert!(massing(&Model::new(), Kind::Block, CONCRETE)
            .mesh
            .indices
            .is_empty());
    }
}
