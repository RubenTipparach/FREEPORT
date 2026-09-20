use super::*;
use crate::field::{Density, Planet, TERRAIN};
use crate::town::{self, arm, frame_at, Piece, BLOCK};

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

/// Every triangle of a model as its material and its three corners, in
/// `f64`, which is what a cross section is measured off.
fn faces(m: &Model) -> Vec<(u8, [DVec3; 3])> {
    m.mesh
        .indices
        .chunks(3)
        .zip(&m.mesh.materials)
        .map(|(ix, &mat)| {
            let p = |k: usize| {
                let q = m.mesh.positions[ix[k] as usize];
                DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)
            };
            (mat, [p(0), p(1), p(2)])
        })
        .collect()
}

/// A mesh's positions are `f32`, so a height is held to about a ten
/// millionth of a metre and never to the bit.
const CLOSE: f64 = 1e-5;

/// Whether a triangle lies flat at height `up`.
fn lies_at(t: &[DVec3; 3], up: f64) -> bool {
    t.iter().all(|p| (p.z - up).abs() < CLOSE)
}

/// How much ground a material covers at height `up`, square metres.
/// Every panel of a street is a horizontal quad, so an area in that
/// plane is what a cross section actually claims.
fn flat_area(m: &Model, material: u8, up: f64) -> f64 {
    faces(m)
        .iter()
        .filter(|(mat, t)| *mat == material && lies_at(t, up))
        .map(|(_, t)| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5)
        .sum()
}

/// How far east a material reaches at height `up`: the nearest its
/// corners come to the centreline and the furthest they go from it,
/// both as distances, so a pair of strips either side reads as one.
fn span(m: &Model, material: u8, up: f64) -> (f64, f64) {
    let mut lo = f64::MAX;
    let mut hi: f64 = 0.0;
    for (mat, t) in faces(m) {
        if mat != material || !lies_at(&t, up) {
            continue;
        }
        for p in t {
            lo = lo.min(p.x.abs());
            hi = hi.max(p.x.abs());
        }
    }
    (lo, hi)
}

/// A street is a CROSS SECTION and not a box: two lanes of carriageway
/// with a raised pavement either side, and its markings on top.
///
/// This is mining-mike's own lesson, and what it replaces is the single
/// four metre quad this was: no kerb, no pavement, no markings, and too
/// narrow for two cars besides.
#[test]
fn a_street_is_two_lanes_between_two_pavements() {
    let p = Piece {
        x: 0.0,
        z: 0.0,
        w: town::STREET,
        d: town::PIECE,
        arms: 0,
    };
    let m = street(&p);
    assert!(m.lamps.is_empty(), "paving carries a lamp");
    // The pavement is what a body stands ON and the carriageway is not:
    // five centimetres is under anything and twelve is ankle deep.
    assert_eq!(m.solids.len(), 2, "a run's two pavements are not two boxes");
    for s in &m.solids {
        assert_eq!(s.material, CONCRETE);
        let top = s.centre.z + s.half.z;
        assert!(
            (top - (LIFT + town::KERB)).abs() < CLOSE,
            "a kerb tops out at {top:.3} m, not {:.3}",
            LIFT + town::KERB
        );
        assert!(
            top < 0.6,
            "a kerb is {top:.3} m, which is past the walker's own step"
        );
    }
    let top = LIFT + town::KERB;
    // The carriageway is flat at the paving's own height and is exactly
    // the two lanes across, measured from its own middle.
    let (road_lo, road_hi) = span(&m, STREET, LIFT);
    assert!(
        (road_lo - town::LANE).abs() < CLOSE && (road_hi - town::LANE).abs() < CLOSE,
        "the carriageway runs {road_lo:.3} to {road_hi:.3} m out, not one quad to {:.3}",
        town::LANE
    );
    assert!(
        (flat_area(&m, STREET, LIFT) - 2.0 * town::LANE * town::PIECE).abs() < CLOSE,
        "the carriageway is not both lanes wide"
    );
    // The pavement stands a kerb over it and runs from the lane's edge
    // out to the street's own, so the two meet with nothing between.
    let (walk_lo, walk_hi) = span(&m, CONCRETE, top);
    assert!(
        (walk_lo - town::LANE).abs() < CLOSE && (walk_hi - town::STREET * 0.5).abs() < CLOSE,
        "the pavement runs {walk_lo:.3} to {walk_hi:.3} m out, not {:.3} to {:.3}",
        town::LANE,
        town::STREET * 0.5
    );
    // Two cars pass with better than a metre between them, and two
    // people pass on the pavement.
    assert!(town::LANE - 1.6 > 1.0, "two cars cannot pass");
    assert!(town::WALK - 0.45 > 0.45, "two people cannot pass");
    // And the markings are PAINT, over the tarmac and under the kerb.
    let paint: Vec<_> = faces(&m).into_iter().filter(|(x, _)| *x == PAINT).collect();
    assert!(!paint.is_empty(), "a street with no markings on it");
    for (_, t) in &paint {
        for q in t {
            assert!(
                q.z > LIFT + CLOSE && q.z < top,
                "a marking at {:.4} is not on the road",
                q.z
            );
            assert!(q.x.abs() < town::LANE, "a marking off the carriageway");
        }
    }
}

/// A CROSSING owns its whole square, which is what a run stopping short
/// of one leaves it room to do. What it is paved with is its ARMS: a
/// crossroads is a plus of tarmac with four kerbed corners, and a DEAD
/// END closes with a pavement across it rather than stopping mid cell
/// with an open edge.
#[test]
fn a_crossing_is_paved_by_its_arms_and_a_dead_end_closes() {
    let square = |arms: u8| {
        street(&Piece {
            x: 0.0,
            z: 0.0,
            w: town::STREET,
            d: town::STREET,
            arms,
        })
    };
    let whole = town::STREET * town::STREET;
    let centre = 2.0 * town::LANE * (2.0 * town::LANE);
    let band = 2.0 * town::LANE * town::WALK;
    for (arms, bands, name) in [
        (
            arm::NORTH | arm::SOUTH | arm::EAST | arm::WEST,
            4.0,
            "a crossroads",
        ),
        (arm::NORTH | arm::EAST, 2.0, "a bend"),
        (arm::NORTH | arm::SOUTH, 2.0, "a street through"),
        (arm::NORTH, 1.0, "a dead end"),
    ] {
        let m = square(arms);
        let road = flat_area(&m, STREET, LIFT);
        let walk = flat_area(&m, CONCRETE, LIFT + town::KERB);
        let want = centre + bands * band;
        println!("{name}: {road:.2} m2 of tarmac and {walk:.2} of pavement");
        assert!(
            (road - want).abs() < CLOSE,
            "{name} is {road:.2} m2 of tarmac, not {want:.2}"
        );
        // Every square metre of it is one or the other, and never both.
        assert!(
            (road + walk - whole).abs() < CLOSE,
            "{name} covers {:.2} m2 of its own {whole:.2}",
            road + walk
        );
        // Its pavement is stood on and its tarmac is not, so a crossing
        // carries one box a cell that is not carriageway.
        let walks = 9 - (1.0 + bands) as usize;
        assert_eq!(m.solids.len(), walks, "{name} is not {walks} pavements");
    }
    // The dead end closes: nothing south of the middle cell is tarmac.
    let m = square(arm::NORTH);
    for (mat, t) in faces(&m) {
        if mat == STREET {
            assert!(
                t.iter().all(|p| p.y >= -town::LANE - CLOSE),
                "the dead end runs out past its own kerb"
            );
        }
    }
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
        sites: vec![].into(),
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
    // Past the nominal radius by the LOBES and the STRETCH, which is how
    // far a town's own outline can actually run.
    let reach = (town.radius * town::OUTLINE + town::STREET + BLOCK) as f32;
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
/// A walker STANDS ON the pavement rather than in it, which is what the
/// kerb being a box a body can stand on buys.
///
/// Five centimetres of carriageway is under anything and needs no
/// collider; a twelve centimetre kerb is ankle deep, so a pavement that
/// only drew would be one a walker waded along. It is still well under
/// his own sixty centimetre step, so he walks up onto it rather than
/// being stopped by it.
#[test]
fn a_walker_stands_on_the_kerb_and_steps_up_onto_it() {
    let mut planet = Planet {
        radius: 4_000.0,
        relief: 24.0,
        lumps: 12.0,
        octaves: 9,
        overhang: 0.0,
        ledge: 0.0,
        seed: 11,
        sites: vec![].into(),
    };
    let towns = town::plan(&planet, planet.radius - 8.0, 40.0, 1, 11);
    let town = towns[0].clone();
    planet.sites = towns.iter().map(town::site_of).collect();
    let fab = fabric(&town, planet.radius, 11);
    let level = planet.radius + town.h;
    // A run the town actually laid, rather than a place a street was
    // guessed to be: the pieces are where the paving IS.
    let run = town
        .pieces
        .iter()
        .find(|p| p.run() && p.northerly())
        .expect("the town laid no run of street");
    let stand = |x: f64, frames: usize| {
        let frame = town::lot_frame(planet.radius, &town, run.x + x, run.z);
        let here = frame.world(DVec3::new(0.0, 0.0, 1.7));
        let north = frame.world(DVec3::new(0.0, 10.0, 1.7)) - here;
        let (w, _) = walk_town(&planet, &fab.blocks, here, north, frames);
        (w.on_ground, w.foot - level)
    };
    // On the pavement, where a pedestrian keeps: the middle of it.
    let (on_ground, over) = stand(town::LANE + town::WALK * 0.5, 30);
    println!("the feet stand {over:.3} m over the town's level on the pavement");
    assert!(on_ground, "the walker is airborne on a pavement");
    assert!(
        (over - (LIFT + town::KERB)).abs() < 0.05,
        "the feet stand {over:.3} m up, not the {:.3} a kerb is",
        LIFT + town::KERB
    );
    // And the ground ACROSS a street is the cross section underfoot:
    // carriageway out to the lane's own edge, then a kerb to the
    // street's. The MIDDLE of each of eighteen bands across the half
    // street, so no sample falls exactly on the kerb's own line, where
    // either answer is right and the rounding would pick.
    let mut seen = Vec::new();
    for k in 0..18 {
        let x = (k as f64 + 0.5) * town::STREET * 0.5 / 18.0;
        let (_, over) = stand(x, 30);
        let want = if x < town::LANE {
            0.0
        } else {
            LIFT + town::KERB
        };
        seen.push(format!("{x:.2}:{over:.3}"));
        assert!(
            (over - want).abs() < 0.05,
            "{x:.2} m off the centreline the feet stand {over:.3} m up, not {want:.3}"
        );
    }
    println!(
        "across the street, metres out against metres up: {}",
        seen.join(" ")
    );
}

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
        sites: vec![].into(),
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

/// A town is not ONE GREY. Every house is wood, brick or vinyl and every
/// office is brick, concrete, marble, glass or stone, which is the
/// owner's own list; and a building's skin is a function of its SEED
/// alone, so a town built as the eye arrives and dropped as it leaves is
/// the same town when you drive back into it.
#[test]
fn a_house_and_an_office_are_built_of_different_trades() {
    use crate::field::{BRICK, CONCRETE, CURTAIN, MARBLE, STONE, VINYL, WOOD};
    let houses = [WOOD, BRICK, VINYL];
    let offices = [BRICK, CONCRETE, MARBLE, CURTAIN, STONE];
    let mut seen: Vec<(Kind, Vec<u8>)> = Vec::new();
    for kind in Kind::all() {
        let mut had: Vec<u8> = Vec::new();
        for seed in 0..600u32 {
            let skin = kind.skin(seed);
            assert_eq!(
                skin,
                kind.skin(seed),
                "{} is not a function of its seed",
                kind.name()
            );
            let allowed: &[u8] = match kind {
                Kind::House | Kind::Bungalow => &houses,
                Kind::Block | Kind::Tower => &offices,
                Kind::Hangar => &[CONCRETE],
            };
            assert!(
                allowed.contains(&skin),
                "a {} came out of trade {skin}",
                kind.name()
            );
            if !had.contains(&skin) {
                had.push(skin);
            }
        }
        had.sort_unstable();
        seen.push((kind, had));
    }
    for (kind, had) in &seen {
        let want = match kind {
            Kind::House | Kind::Bungalow => 3,
            Kind::Block | Kind::Tower => 5,
            Kind::Hangar => 1,
        };
        assert_eq!(
            had.len(),
            want,
            "{} used {had:?} of {want} trades",
            kind.name()
        );
    }
    // And a house is never built of an office's stone, which is the
    // whole of what the two lists are for.
    let house = &seen
        .iter()
        .find(|(k, _)| *k == Kind::House)
        .expect("a house")
        .1;
    assert!(!house.contains(&MARBLE) && !house.contains(&STONE) && !house.contains(&CURTAIN));
}

/// The walls wear the skin and the FLOOR does not: a slab is poured
/// concrete whatever is hung off the outside of the building.
#[test]
fn a_walls_skin_is_on_the_wall_and_not_on_the_floor() {
    use crate::field::{CONCRETE, WOOD};
    // A seed whose house comes out timber, found rather than assumed.
    let seed = (0..600u32)
        .find(|s| Kind::House.skin(*s) == WOOD)
        .expect("some seed builds a timber house");
    let m = building(Kind::House, BLOCK, BLOCK, 1, seed);
    let mut wall = 0;
    let mut floor = 0;
    for b in &m.solids {
        if b.material == WOOD {
            wall += 1;
        }
        if b.material == CONCRETE {
            floor += 1;
            // The one concrete box in a house is its slab, which is the
            // widest thing in it and sits at the bottom.
            assert!(
                b.half.z < 0.5,
                "a concrete box {} high is not a floor",
                b.half.z
            );
        }
    }
    assert!(wall >= 4, "{wall} timber boxes is not four walls");
    assert_eq!(floor, 1, "{floor} concrete boxes in a timber house");
}
