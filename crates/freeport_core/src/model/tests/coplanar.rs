//! Faces that fight for one plane: what the owner saw as z-fighting,
//! measured on the models rather than looked for in a picture.

use super::super::*;
use crate::town::{arm, Piece};

/// Two faces facing the same way in the same plane where they overlap
/// and where a camera can SEE them: every such pair of a model's
/// triangles, as the overlap's area. Hidden is a face pressed down on the
/// ground, or one with another of the model's solids standing on it,
/// which is what every wall's top under a roof slab is.
fn coplanar_overlaps(m: &Model) -> Vec<(usize, usize, f64)> {
    let p = |i: u32| {
        let v = m.mesh.positions[i as usize];
        DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
    };
    let tris: Vec<[DVec3; 3]> = m
        .mesh
        .indices
        .chunks(3)
        .map(|t| [p(t[0]), p(t[1]), p(t[2])])
        .collect();
    let normal = |t: &[DVec3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
    let mut out = Vec::new();
    for i in 0..tris.len() {
        let n = normal(&tris[i]);
        if n == DVec3::ZERO {
            continue;
        }
        let (u, v) = (
            n.any_orthonormal_vector(),
            n.cross(n.any_orthonormal_vector()),
        );
        let flat = |t: &[DVec3; 3]| t.map(|q| DVec2::new(q.dot(u), q.dot(v)));
        for j in i + 1..tris.len() {
            if n.dot(normal(&tris[j])) < 0.9999 || n.dot(tris[j][0] - tris[i][0]).abs() > 1e-3 {
                continue;
            }
            let Some((area, middle)) = overlap(&flat(&tris[i]), &flat(&tris[j])) else {
                continue;
            };
            let at = u * middle.x + v * middle.y + n * n.dot(tris[i][0]);
            let grounded = n.z < -0.99 && at.z < 0.01;
            if area > 1e-4 && !grounded && !buried(m, at + n * 0.005) {
                out.push((i, j, area));
            }
        }
    }
    out
}

/// Whether a point is inside one of a model's solids.
fn buried(m: &Model, q: DVec3) -> bool {
    m.solids.iter().any(|s| {
        let d = q - s.centre;
        let a = s.axes();
        (0..3).all(|k| d.dot(a[k]).abs() < s.half[k] - 1e-4)
    })
}

/// The area two triangles in one plane both cover and its middle: one
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

/// The owner saw z-fighting on the buildings: pillars flush with the walls
/// for a tower's whole height, a floor slab's sides in the walls' own
/// planes, gable ends reaching into the next roof. Two faces in one plane
/// where they overlap and can be seen are two faces fighting for every
/// pixel there, so no building of any kind has one.
#[test]
fn no_two_faces_of_a_building_fight_for_one_plane() {
    for kind in Kind::all() {
        for (w, storeys) in [(10.0, 2), (20.0, 5)] {
            let m = building(kind, w, w, storeys, 7);
            let found = coplanar_overlaps(&m);
            assert!(
                found.is_empty(),
                "{kind:?} {w} m, {storeys} storeys: {} coplanar pairs, the first {:?}",
                found.len(),
                found.first()
            );
        }
    }
}

/// And the paving: the owner saw it on the pavements too. Every piece a
/// town lays (a run either way, a crossing with any arms, the square) at
/// every grade of detail it is drawn at.
#[test]
fn no_two_faces_of_a_street_fight_for_one_plane() {
    let mut pieces = vec![
        Piece {
            x: 0.0,
            z: 0.0,
            w: 40.0,
            d: 8.5,
            arms: 0,
        },
        Piece {
            x: 0.0,
            z: 0.0,
            w: 8.5,
            d: 40.0,
            arms: 0,
        },
        Piece {
            x: 0.0,
            z: 0.0,
            w: 97.0,
            d: 97.0,
            arms: arm::SQUARE,
        },
    ];
    for arms in 1..16u8 {
        pieces.push(Piece {
            x: 0.0,
            z: 0.0,
            w: 8.5,
            d: 8.5,
            arms,
        });
    }
    for piece in &pieces {
        for grade in 0..4 {
            let found = coplanar_overlaps(&street::street_graded(piece, grade));
            assert!(
                found.is_empty(),
                "arms {:#06b} at grade {grade}: {} coplanar pairs, the first {:?}",
                piece.arms,
                found.len(),
                found.first()
            );
        }
    }
}
