//! Faces that fight for one plane where two things MEET: a piece of street
//! beside the next, a crossing beside its runs, a building beside its
//! neighbour on graded ground. The coplanar tests hold each model on its
//! own; this is the whole of a town laid as the game lays it.

use super::super::*;
use crate::field::Planet;
use crate::fights::{in_town, Fight};

/// The harness body and the port a plan puts on it.
fn port() -> (Planet, Town) {
    let p = Planet {
        radius: 1_000_000.0,
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        seed: 7,
        sites: vec![].into(),
    };
    let sea = p.radius + 1_100.0;
    let t = crate::town::plan(&p, sea, 537.0, 1, 7)
        .into_iter()
        .next()
        .expect("a port on the harness body");
    (p, t)
}

/// What meets what, for a message: the two materials and which way the
/// plane faces, with how many fights and over how much.
fn tally(mesh: &DcMesh, found: &[Fight]) -> String {
    let mut kinds: std::collections::BTreeMap<(u8, u8, bool), (usize, f64)> = Default::default();
    for f in found {
        let (m, n) = (mesh.materials[f.a], mesh.materials[f.b]);
        let v = |c: usize| {
            DVec3::from(mesh.positions[mesh.indices[f.a * 3 + c] as usize].map(f64::from))
        };
        let up = (v(1) - v(0)).cross(v(2) - v(0)).normalize_or_zero().z > 0.9;
        let e = kinds.entry((m.min(n), m.max(n), up)).or_default();
        e.0 += 1;
        e.1 += f.area;
    }
    kinds
        .iter()
        .map(|((m, n, up), (c, a))| {
            let facing = if *up { "up" } else { "side" };
            format!("materials {m} and {n} facing {facing}: {c} over {a:.3} m^2")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The owner saw z-fighting on the pavements as well as the buildings,
/// and a piece of street is only half of a pavement: the other half is
/// the piece beside it, the crossing it runs into and the building at
/// its back. So the whole port, every lot and every piece laid on its
/// graded ground, has no two faces in one plane where they overlap and
/// can be seen. It found two things the models alone could not: a road
/// marking laid four millimetres over a carriageway whose draped chord
/// moves more than that (`model::street`'s `PAINT_UP`), and a gable's
/// end standing out over its eaves into the wall of the lot behind it.
#[test]
fn no_two_faces_of_a_town_fight_for_one_plane() {
    let (p, t) = port();
    let f = fabric(&t, p.radius, 7);
    let found = in_town(&f, &lot_frame(p.radius, &t, 0.0, 0.0));
    println!(
        "{} lots and {} pieces, {} triangles: {} fights ({})",
        f.buildings,
        f.pieces,
        f.mesh.indices.len() / 3,
        found.len(),
        tally(&f.mesh, &found)
    );
    assert!(found.is_empty(), "{} fights in the port", found.len());
}

/// How far a street's triangles leave the graded ground they are draped
/// on: each is laid at its own corners, so its middle is a chord of the
/// curving grade. What `PAINT_UP` is set against.
#[test]
#[ignore = "measurement, run with --ignored --nocapture"]
fn measure_how_far_a_street_chord_leaves_its_ground() {
    let (p, t) = port();
    let g = t.grade.as_ref().expect("a planned town is graded");
    let f = fabric(&t, p.radius, 7);
    let middle = lot_frame(p.radius, &t, 0.0, 0.0);
    let over = |q: DVec3| {
        let w = middle.world(q);
        w.length() - p.radius - g.at_dir(w.normalize())
    };
    let (mut corner, mut mid) = ((f64::MAX, f64::MIN), (f64::MAX, f64::MIN));
    let (mut longest, mut count) = (0.0f64, 0);
    for (k, tri) in f.mesh.indices.chunks(3).enumerate() {
        let v: Vec<DVec3> = tri
            .iter()
            .map(|&i| DVec3::from(f.mesh.positions[i as usize].map(f64::from)))
            .collect();
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        if f.mesh.materials[k] != crate::field::STREET || n.z < 0.9 {
            continue;
        }
        count += 1;
        for q in &v {
            corner = (corner.0.min(over(*q)), corner.1.max(over(*q)));
        }
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            longest = longest.max((v[a] - v[b]).length());
        }
        let h = over((v[0] + v[1] + v[2]) / 3.0);
        mid = (mid.0.min(h), mid.1.max(h));
    }
    println!(
        "{count} upward street triangles, the longest edge {longest:.1} m: corners {:.3} to {:.3} m over the ground, middles {:.3} to {:.3}",
        corner.0, corner.1, mid.0, mid.1
    );
}
