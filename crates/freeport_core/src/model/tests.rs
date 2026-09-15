use super::*;
use crate::field::{Density, Planet, TERRAIN};
use crate::town::{self, frame_at, BLOCK};

/// A frame at the pole of a ball, which is where a model is measured.
fn frame_on(radius: f64) -> Frame {
    let (east, north) = frame_at(DVec3::Y);
    Frame {
        dir: DVec3::Y,
        east,
        north,
        base: radius,
    }
}

/// Whether any of a model's boxes holds a point of its own frame.
fn inside(m: &Model, frame: &Frame, local: DVec3) -> bool {
    let p = frame.world(local);
    m.blocks(frame).iter().any(|b| b.at(p) > 0.0)
}

#[test]
fn a_wall_stops_a_body_and_the_doorway_does_not() {
    let m = building(Kind::House, BLOCK, BLOCK, 2, 11);
    let frame = frame_on(2_000.0);
    let half = BLOCK * 0.5;
    // In the middle of the north wall, at head height: rock.
    assert!(
        inside(&m, &frame, DVec3::new(0.0, half - WALL * 0.5, 1.6)),
        "the north wall is not there"
    );
    // The same height in the east and west walls.
    for side in [-1.0, 1.0] {
        assert!(
            inside(&m, &frame, DVec3::new(side * (half - WALL * 0.5), 0.0, 1.6)),
            "a side wall is not there"
        );
    }
    // The doorway is a way IN: every point of a walk from two metres
    // outside it to the middle of the room, at a walker's chest, is air.
    for k in 0..=40 {
        let t = k as f64 / 40.0;
        let y = -half - 2.0 + t * (half + 2.0);
        assert!(
            !inside(&m, &frame, DVec3::new(0.0, y, 1.6)),
            "the doorway is walled up {y:.2} m north of the lot's middle"
        );
    }
    // And the wall either side of it is not a way in.
    assert!(
        inside(&m, &frame, DVec3::new(DOOR_W, -(half - WALL * 0.5), 1.6)),
        "the pier beside the door is missing"
    );
    // Over the door there is a lintel, so the doorway is a doorway.
    assert!(
        inside(
            &m,
            &frame,
            DVec3::new(0.0, -(half - WALL * 0.5), DOOR_H + 0.5)
        ),
        "the lintel over the door is missing"
    );
}

#[test]
fn every_kind_builds_a_mesh_and_the_boxes_that_stop_a_body() {
    for kind in Kind::all() {
        let (low, _) = kind.storeys();
        let m = building(kind, BLOCK, BLOCK, low, 3);
        let name = kind.name();
        assert!(
            m.mesh.triangles() > 20,
            "{name} is {} triangles",
            m.mesh.triangles()
        );
        assert!(!m.solids.is_empty(), "{name} stops nothing");
        assert!(!m.lamps.is_empty(), "{name} is unlit");
        assert_eq!(
            m.mesh.materials.len(),
            m.mesh.triangles(),
            "{name} has a material per triangle"
        );
        // Nothing stands outside its own lot, and nothing hangs under the
        // ground: a model that did would meet its neighbour or the field.
        let reach = BLOCK * 0.5 + 0.5;
        for p in &m.mesh.positions {
            assert!(
                p[0].abs() <= reach as f32 && p[1].abs() <= reach as f32,
                "{name} reaches {p:?}, past its own lot"
            );
            assert!(p[2] >= -0.01, "{name} hangs under the ground at {p:?}");
        }
        assert!(
            m.high() > low as f64 * STOREY,
            "{name} is not a storey tall"
        );
    }
}

#[test]
fn a_streets_paving_lies_over_the_ground_and_stops_nothing() {
    let m = street(town::STREET, town::PIECE);
    assert_eq!(m.mesh.triangles(), 2, "a piece of street is one quad");
    assert!(m.solids.is_empty(), "paving stops a body");
    assert!(m.lamps.is_empty(), "paving carries a lamp");
    for p in &m.mesh.positions {
        assert!(
            (p[2] as f64 - LIFT).abs() < 1e-6,
            "the paving is not flat: {p:?}"
        );
    }
    assert_eq!(m.mesh.materials, vec![STREET, STREET]);
}

#[test]
fn a_pane_is_glass_or_lit_and_never_in_the_doorway() {
    let m = building(Kind::Block, BLOCK, BLOCK, 5, 7);
    let panes = m
        .mesh
        .materials
        .iter()
        .filter(|&&x| x == GLASS || x == LIT)
        .count();
    assert!(panes > 8, "a five storey block has {panes} pane triangles");
    assert!(
        m.mesh.materials.contains(&CONCRETE) && m.mesh.materials.contains(&PLATE),
        "a block is concrete with plate on it"
    );
}

#[test]
fn a_towns_fabric_is_one_mesh_in_its_own_frame() {
    let planet = Planet {
        radius: 4_000.0,
        relief: 24.0,
        lumps: 12.0,
        octaves: 9,
        overhang: 0.0,
        ledge: 0.0,
        seed: 11,
        sites: vec![],
    };
    let towns = town::plan(&planet, planet.radius - 8.0, 40.0, 1, 11);
    assert!(!towns.is_empty(), "the ball grew no town");
    let town = &towns[0];
    let f = fabric(town, planet.radius, 11);
    assert_eq!(f.buildings, town.lots.len());
    assert_eq!(f.pieces, town.pieces.len());
    assert!(
        f.mesh.triangles() > 1_000,
        "{} triangles",
        f.mesh.triangles()
    );
    assert!(!f.blocks.is_empty(), "the town stops nothing");
    assert!(
        f.lamps.len() >= f.buildings * 2,
        "{} lamps over {} buildings, which is under one at the door and          one inside",
        f.lamps.len(),
        f.buildings
    );
    // Every vertex is in the town's own frame, so an f32 holds a micron:
    // nothing is further from the middle than the town is wide.
    let reach = (town.radius + town::STREET + BLOCK) as f32;
    for p in &f.mesh.positions {
        assert!(
            p[0].abs() < reach && p[1].abs() < reach,
            "a vertex at {p:?} is outside a town of {reach} m"
        );
    }
    // The boxes are in the WORLD, on the planet's own surface.
    for b in &f.blocks {
        let r = b.centre.length();
        assert!(
            (r - planet.radius).abs() < 200.0,
            "a box {r:.0} m from the centre of a {:.0} m planet",
            planet.radius
        );
    }
    // Nothing built is terrain: the ground is the field's and the models
    // are what stands on it.
    assert!(!f.mesh.materials.contains(&TERRAIN));
}
