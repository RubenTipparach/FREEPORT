//! Faces that fight for one plane: what the owner saw as z-fighting,
//! measured on the models rather than looked for in a picture.

use super::super::*;
use crate::town::{arm, Piece};

/// Two faces facing the same way in the same plane where they overlap
/// and where a camera can SEE them (`fights::fights`): every such pair of
/// a model's triangles. Hidden is a face pressed down on the ground, or
/// one with another of the model's solids standing on it, which is what
/// every wall's top under a roof slab is.
fn coplanar_overlaps(m: &Model) -> Vec<crate::fights::Fight> {
    crate::fights::fights(&m.mesh, |at, n| {
        let grounded = n.z < -0.99 && at.z < 0.01;
        grounded || buried(m, at + n * 0.005)
    })
}

/// Whether a point is inside one of a model's solids.
fn buried(m: &Model, q: DVec3) -> bool {
    m.solids.iter().any(|s| {
        let d = q - s.centre;
        let a = s.axes();
        (0..3).all(|k| d.dot(a[k]).abs() < s.half[k] - 1e-4)
    })
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
