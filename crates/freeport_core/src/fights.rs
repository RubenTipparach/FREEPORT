//! Faces that fight for one plane: z-fighting, measured on a mesh rather
//! than looked for in a picture.
//!
//! Two triangles facing the same way within a millimetre of one plane,
//! where they overlap, are two surfaces the depth buffer cannot order:
//! each pixel of the overlap goes to whichever the rasteriser's rounding
//! favours, and the pattern moves as the camera does. Near the eye that
//! is not precision (an `f32` two kilometres from the origin holds a
//! quarter of a millimetre); it is geometry that really lies in one
//! place. What the caller says is HIDDEN (a face pressed on the ground,
//! one inside another solid) is not a fight, because nobody sees it.

use crate::dc::DcMesh;
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// One fight: the two triangles (by index), how much of them overlaps,
/// square metres, and the middle of the overlap in the mesh's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fight {
    pub a: usize,
    pub b: usize,
    pub area: f64,
    pub at: DVec3,
}

/// Every fight in a mesh. Triangles are binned by their plane first (the
/// normal to a hundredth, the distance from the frame's middle to a
/// centimetre, and each bin checked against the next distance along so a
/// rounding cannot split a pair), so a town of hundreds of thousands of
/// triangles is checked in a second. `hidden` is asked, of the overlap's
/// middle and the plane's normal, whether anybody can see it.
pub fn fights(mesh: &DcMesh, hidden: impl Fn(DVec3, DVec3) -> bool) -> Vec<Fight> {
    let p = |i: u32| DVec3::from(mesh.positions[i as usize].map(f64::from));
    let tris: Vec<[DVec3; 3]> = mesh
        .indices
        .chunks(3)
        .map(|t| [p(t[0]), p(t[1]), p(t[2])])
        .collect();
    let normals: Vec<DVec3> = tris
        .iter()
        .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero())
        .collect();
    let q = |v: f64| (v * 100.0).round() as i64;
    let mut bins: BTreeMap<([i64; 3], i64), Vec<usize>> = BTreeMap::new();
    for (i, (t, n)) in tris.iter().zip(&normals).enumerate() {
        if *n != DVec3::ZERO {
            let key = ([q(n.x), q(n.y), q(n.z)], q(n.dot(t[0])));
            bins.entry(key).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    for (key, members) in &bins {
        let next = bins.get(&(key.0, key.1 + 1));
        for (k, &a) in members.iter().enumerate() {
            for &b in members[k + 1..].iter().chain(next.into_iter().flatten()) {
                let Some((area, at)) = fight(&tris[a], &tris[b], normals[a], normals[b]) else {
                    continue;
                };
                if !hidden(at, normals[a]) {
                    out.push(Fight { a, b, area, at });
                }
            }
        }
    }
    out
}

/// Whether two triangles lie in one plane facing one way and overlap
/// there by more than a square centimetre, and where and by how much.
fn fight(a: &[DVec3; 3], b: &[DVec3; 3], na: DVec3, nb: DVec3) -> Option<(f64, DVec3)> {
    if na.dot(nb) < 0.9999 || na.dot(b[0] - a[0]).abs() > 1e-3 {
        return None;
    }
    let lo = |t: &[DVec3; 3]| t[0].min(t[1]).min(t[2]);
    let hi = |t: &[DVec3; 3]| t[0].max(t[1]).max(t[2]);
    if lo(a).cmpgt(hi(b)).any() || lo(b).cmpgt(hi(a)).any() {
        return None;
    }
    let u = na.any_orthonormal_vector();
    let v = na.cross(u);
    let flat = |t: &[DVec3; 3]| t.map(|q| DVec2::new(q.dot(u), q.dot(v)));
    let (area, middle) = overlap(&flat(a), &flat(b))?;
    (area > 1e-4).then(|| (area, u * middle.x + v * middle.y + na * na.dot(a[0])))
}

/// The area two triangles in one plane both cover, and its middle: one
/// clipped by the other's three edges (Sutherland and Hodgman), then the
/// shoelace. Nothing when they do not overlap.
fn overlap(a: &[DVec2; 3], b: &[DVec2; 3]) -> Option<(f64, DVec2)> {
    let ccw = |t: &[DVec2; 3]| {
        if (t[1] - t[0]).perp_dot(t[2] - t[0]) > 0.0 {
            t.to_vec()
        } else {
            t.iter().rev().copied().collect()
        }
    };
    let (mut poly, clip) = (ccw(a), ccw(b));
    for k in 0..3 {
        let (e0, e1) = (clip[k], clip[(k + 1) % 3]);
        let side = |q: DVec2| (e1 - e0).perp_dot(q - e0);
        let mut next = Vec::new();
        for n in 0..poly.len() {
            let (p0, p1) = (poly[n], poly[(n + 1) % poly.len()]);
            let (d0, d1) = (side(p0), side(p1));
            if d0 > 1e-9 {
                next.push(p0);
            }
            if (d0 > 1e-9) != (d1 > 1e-9) {
                next.push(p0 + (p1 - p0) * (d0 / (d0 - d1)));
            }
        }
        poly = next;
        if poly.len() < 3 {
            return None;
        }
    }
    let area = (0..poly.len())
        .map(|n| poly[n].perp_dot(poly[(n + 1) % poly.len()]))
        .sum::<f64>()
        .abs()
        * 0.5;
    let middle = poly.iter().copied().sum::<DVec2>() / poly.len() as f64;
    Some((area, middle))
}

/// Every fight in a town laid by `model::fabric_part`. Its mesh is in the
/// town's own frame (`middle`, the frame of its middle lot) and its boxes
/// in the planet's, so an overlap is carried out before it is asked
/// whether a box stands on it; and a face turned DOWN is hidden, because
/// only the ground under a slab or a plinth sees it.
pub fn in_town(fabric: &crate::model::Fabric, middle: &crate::town::Frame) -> Vec<Fight> {
    fights(&fabric.mesh, |at, n| {
        let q = middle.world(at + n * 0.005);
        n.z < -0.9
            || fabric.blocks.iter().any(|s| {
                let d = q - s.centre;
                (0..3).all(|k| d.dot(s.axes[k]).abs() < s.half[k] - 1e-4)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad of two triangles from four corners, facing up.
    fn quad(m: &mut DcMesh, lo: (f64, f64), hi: (f64, f64), z: f64) {
        let base = m.positions.len() as u32;
        for (x, y) in [(lo.0, lo.1), (hi.0, lo.1), (hi.0, hi.1), (lo.0, hi.1)] {
            m.positions.push([x as f32, y as f32, z as f32]);
            m.normals.push([0.0, 0.0, 1.0]);
            m.levels.push(0);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        m.materials.extend([0, 0]);
    }

    /// Two squares in one plane that overlap by a quarter fight over
    /// exactly that quarter; lifted a centimetre apart, or laid side by
    /// side, or hidden, they do not.
    #[test]
    fn two_faces_in_one_plane_fight_over_their_overlap_and_nowhere_else() {
        let mut m = DcMesh::default();
        quad(&mut m, (0.0, 0.0), (2.0, 2.0), 0.0);
        quad(&mut m, (1.0, 1.0), (3.0, 3.0), 0.0);
        let found = fights(&m, |_, _| false);
        let area: f64 = found.iter().map(|f| f.area).sum();
        assert!(
            (area - 1.0).abs() < 1e-6,
            "{area} m^2 of {} fights",
            found.len()
        );
        assert!(fights(&m, |_, _| true).is_empty(), "hidden is not a fight");
        let mut apart = DcMesh::default();
        quad(&mut apart, (0.0, 0.0), (2.0, 2.0), 0.0);
        quad(&mut apart, (1.0, 1.0), (3.0, 3.0), 0.01);
        quad(&mut apart, (2.0, 0.0), (4.0, -2.0), 0.0);
        assert!(fights(&apart, |_, _| false).is_empty());
    }
}
