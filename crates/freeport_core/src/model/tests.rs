use super::*;
use crate::field::{Density, Planet, TERRAIN};
use crate::town::{self, frame_at, BLOCK};

#[test]
fn box_faces_point_outward_and_match_collision_faces() {
    let frame = frame_on(1_000_000.0);
    for yaw in [0.0, 0.73, -1.4] {
        let mut m = Model::new();
        let centre = DVec3::new(2.0, -1.0, 3.0);
        m.solid(centre, DVec3::new(1.0, 2.0, 3.0), yaw, CONCRETE);
        let block = m.solids[0].block(&frame);
        for tri in m.mesh.indices.chunks_exact(3) {
            let p: Vec<_> = tri
                .iter()
                .map(|&i| glam::Vec3::from(m.mesh.positions[i as usize]).as_dvec3())
                .collect();
            let mid = (p[0] + p[1] + p[2]) / 3.0;
            let normal = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
            assert!(normal.dot(mid - centre) > 0.0);
            assert!(block.at(frame.world(mid + normal * 0.01)) < 0.0);
            assert!(block.at(frame.world(mid - normal * 0.01)) > 0.0);
        }
    }
}

#[test]
fn panes_face_out_on_every_wall() {
    for out in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y] {
        let mut m = Model::new();
        let wide = if out.x == 0.0 { DVec3::X } else { DVec3::Y };
        m.pane(out * 5.0, out, wide, PANE_H, 0.5);
        for normal in m.mesh.normals {
            assert!(glam::Vec3::from(normal).as_dvec3().dot(out) > 0.999);
        }
    }
}

#[test]
fn supplied_static_models_use_the_same_town_transform() {
    let planet = Planet {
        radius: 2000.0,
        relief: 100.0,
        octaves: 5,
        ..Planet::default()
    };
    let towns = town::plan(&planet, 1980.0, 40.0, 1, 7);
    let town = towns.first().expect("a test town");
    let ordinary = fabric(town, planet.radius, 7);
    let supplied = fabric_with(town, planet.radius, |lot| {
        building(lot.kind, BLOCK, BLOCK, lot.storeys, 7 ^ lot.id)
    });
    assert_eq!(ordinary.mesh.positions, supplied.mesh.positions);
    assert_eq!(ordinary.mesh.indices, supplied.mesh.indices);
    assert_eq!(ordinary.blocks.len(), supplied.blocks.len());
}

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

/// A walker set down on a street of a town, walking `frames` sixtieths in
/// its heading: where it ends up, and how far it came along the ground.
fn walk_town(
    planet: &Planet,
    blocks: &[crate::field::Block],
    at: DVec3,
    heading: DVec3,
    frames: usize,
) -> (crate::walker::Walker, f64) {
    let field = crate::field::Built {
        ground: planet,
        blocks: blocks.iter().collect(),
    };
    let (floor, roof) = planet.band();
    let bounds = crate::walker::Bounds {
        radius: planet.radius,
        floor: floor - 2.0,
        top: roof + 40.0,
        sea: 0.0,
    };
    let mut w = crate::walker::Walker::enter(&field, &bounds, at, heading);
    let from = w.dir;
    let input = crate::walker::Input {
        forward: 1.0,
        ..Default::default()
    };
    for _ in 0..frames {
        w.update(&field, &bounds, &input, 1.0 / 60.0);
    }
    let gone = from.angle_between(w.dir) * planet.radius;
    (w, gone)
}

/// The town's models are what a body meets: a walker set down on a street
/// walks along it, and one set down facing a lot's wall is stopped by it
/// without being pushed through the ground or thrown off the planet.
///
/// A picture cannot tell a street that is walkable from one a walker is
/// standing in, and a render of this world on a software rasteriser is
/// seventeen minutes, so the walk is measured here where it costs nothing.
#[test]
fn a_walker_walks_a_street_and_is_stopped_by_a_wall() {
    let mut planet = Planet {
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
    let town = towns[0].clone();
    planet.sites = towns.iter().map(town::site_of).collect();
    let fab = fabric(&town, planet.radius, 11);
    // Down the middle of the street west of the town's middle, which is
    // where the harness sets its own walker down.
    let x = -BLOCK / 2.0 - town::STREET / 2.0;
    let frame = town::lot_frame(planet.radius, &town, x, -town.radius * 0.5);
    let start = frame.world(DVec3::new(0.0, 0.0, 1.7));
    let north = frame.world(DVec3::new(0.0, 10.0, 1.7)) - start;
    let (w, gone) = walk_town(&planet, &fab.blocks, start, north, 150);
    assert!(
        (6.0..14.0).contains(&gone),
        "two and a half seconds up the street went {gone:.2} m"
    );
    assert!(w.on_ground, "the walker is airborne on a street");
    let over = w.foot - (planet.radius + town.h);
    assert!(
        (-0.2..0.6).contains(&over),
        "the feet stand {over:.2} m off the town's own level"
    );
    // And a lot's wall stops a body: three metres east of a lot's own east
    // face, walking west into it, the walker gets no further than the face
    // less its own body. From the street it would walk between two lots,
    // because a lot is ten metres on a pitch of fourteen and the gap
    // between two is a way through.
    let lot = &town.lots[town.lots.len() / 2];
    let f = town::lot_frame(planet.radius, &town, lot.x, lot.z);
    let out = BLOCK / 2.0 + 3.0;
    let start = f.world(DVec3::new(out, 0.0, 1.7));
    let west = f.world(DVec3::new(0.0, 0.0, 1.7)) - start;
    let (w, gone) = walk_town(&planet, &fab.blocks, start, west, 150);
    assert!(
        (1.5..3.0).contains(&gone),
        "walked {gone:.2} m at a {} whose wall is 3.0 m away, less a body of 0.35",
        lot.kind.name()
    );
    assert!(w.on_ground, "the walker is airborne against a wall");
}
