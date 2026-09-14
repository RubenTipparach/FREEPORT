//! The vertex of a cell: where the planes through its crossings meet.
//!
//! Dual contouring places one vertex per surface in a cell at the point
//! that best satisfies the planes through that surface's edge crossings,
//! each plane through a crossing's position with its normal. On a flat face
//! the planes agree and the point is free to slide along the face; on an
//! edge two families of planes pin a line; at a corner three pin a point.
//! The solve has to respect exactly that: constrain only the directions the
//! planes constrain, and leave the rest at the crossings' middle.

use glam::DVec3;

/// A crossing on an edge: where the surface is and which way it faces.
#[derive(Clone, Copy, Debug)]
pub struct Crossing {
    /// Where the surface crosses the edge.
    pub p: DVec3,
    /// The field's normal there, pointing out of the rock.
    pub n: DVec3,
}

/// Eigenvalues under this share of the largest are unconstrained
/// directions: a face's normals all agree, so a vertex on it is free to
/// slide along the face and stays at the crossings' middle there.
const RANK_CUTOFF: f64 = 0.1;

/// The least squares point of the planes through the crossings, and their
/// mean normal. Solved from the crossings' middle by the pseudo inverse of
/// the normals' scatter, so the middle moves only in the directions the
/// planes constrain: onto a face, onto an edge, into a corner, and never
/// off the crease toward the middle of the cell, which is what a fixed
/// pull toward the middle did to every box edge (a zigzag strip along the
/// pad's rim, in the first picture). Clamped to a twentieth outside the
/// cell, since three nearly parallel planes can meet a long way off.
pub fn qef(xs: &[Crossing], lo: DVec3, hi: DVec3) -> (DVec3, DVec3) {
    if xs.is_empty() {
        return ((lo + hi) * 0.5, DVec3::Y);
    }
    let mass = xs.iter().map(|x| x.p).sum::<DVec3>() / xs.len() as f64;
    let n = xs.iter().map(|x| x.n).sum::<DVec3>().normalize_or(DVec3::Y);
    let mut a = [[0.0f64; 3]; 3];
    let mut b = DVec3::ZERO;
    for x in xs {
        let q = x.n.to_array();
        for (i, qi) in q.iter().enumerate() {
            for (j, qj) in q.iter().enumerate() {
                a[i][j] += qi * qj;
            }
        }
        b += x.n * x.n.dot(x.p - mass);
    }
    let (values, vectors) = eigen_symmetric(a);
    let largest = values.iter().cloned().fold(0.0f64, f64::max);
    let mut p = mass;
    for k in 0..3 {
        if values[k] > RANK_CUTOFF * largest && values[k] > 1e-12 {
            let v = DVec3::from(vectors[k]);
            p += v * (v.dot(b) / values[k]);
        }
    }
    let slack = (hi - lo) * 0.05;
    (p.clamp(lo - slack, hi + slack), n)
}

/// The eigenvalues and unit eigenvectors of a symmetric 3 by 3 matrix, by
/// Jacobi rotations.
fn eigen_symmetric(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..24 {
        let off = a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2];
        if off < 1e-24 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-18 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for row in a.iter_mut() {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (rp, rq) = (a[p], a[q]);
            for (k, (apk, aqk)) in rp.iter().zip(rq.iter()).enumerate() {
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for row in v.iter_mut() {
                let (vp, vq) = (row[p], row[q]);
                row[p] = c * vp - s * vq;
                row[q] = s * vp + c * vq;
            }
        }
    }
    let values = [a[0][0], a[1][1], a[2][2]];
    // Column k of v is the eigenvector of values[k]; hand them back as rows.
    let vectors = [
        [v[0][0], v[1][0], v[2][0]],
        [v[0][1], v[1][1], v[2][1]],
        [v[0][2], v[1][2], v[2][2]],
    ];
    (values, vectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_least_squares_point_lands_on_a_face_an_edge_and_a_corner() {
        let lo = DVec3::ZERO;
        let hi = DVec3::ONE;
        let x = |p: [f64; 3], n: [f64; 3]| Crossing {
            p: DVec3::from(p),
            n: DVec3::from(n),
        };
        // One face, y = 0.3: the point is the crossings' middle, on the face.
        let face = [
            x([0.0, 0.3, 0.2], [0.0, 1.0, 0.0]),
            x([1.0, 0.3, 0.7], [0.0, 1.0, 0.0]),
            x([0.4, 0.3, 1.0], [0.0, 1.0, 0.0]),
        ];
        let (p, n) = qef(&face, lo, hi);
        assert!((p - DVec3::new(0.4667, 0.3, 0.6333)).length() < 1e-3, "{p}");
        assert!((n - DVec3::Y).length() < 1e-9);
        // An edge where y = 0.3 meets x = 0.6: on the edge, at the middle's z.
        let edge = [
            x([0.0, 0.3, 0.2], [0.0, 1.0, 0.0]),
            x([0.6, 0.0, 0.9], [1.0, 0.0, 0.0]),
            x([0.6, 1.0, 0.4], [1.0, 0.0, 0.0]),
        ];
        let (p, _) = qef(&edge, lo, hi);
        assert!((p - DVec3::new(0.6, 0.3, 0.5)).length() < 1e-9, "{p}");
        // A corner: three planes, one point, wherever the crossings sit.
        let corner = [
            x([0.1, 0.3, 0.9], [0.0, 1.0, 0.0]),
            x([0.6, 0.9, 0.1], [1.0, 0.0, 0.0]),
            x([0.9, 0.1, 0.7], [0.0, 0.0, 1.0]),
        ];
        let (p, _) = qef(&corner, lo, hi);
        assert!((p - DVec3::new(0.6, 0.3, 0.7)).length() < 1e-9, "{p}");
        // Three planes meeting far outside are held to the cell's slack.
        let far = [
            x([0.0, 0.5, 0.5], [0.0, 1.0, 0.0]),
            x([0.5, 0.5, 0.0], [0.0, 0.98, 0.2]),
            x([0.5, 0.5, 1.0], [0.0, 0.98, -0.2]),
        ];
        let (p, _) = qef(&far, lo, hi);
        assert!(p.cmpge(lo - 0.05).all() && p.cmple(hi + 0.05).all(), "{p}");
    }
}
