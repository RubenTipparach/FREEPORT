use super::*;
use crate::buildings::Library;
use crate::city::detail::{stream_tiles, TileState};
use crate::city::stream::{stream_towns, Building};
use crate::city::Glazing;
use crate::stream::Frame as RenderFrame;
use crate::terrain::Ground3d;
use crate::world::{Fabric, World};
use crate::{Eye, Ground};
use bevy::math::DVec2;
use freeport_core::field::Planet;
use freeport_core::model::fabric_with;
use freeport_core::pos::{Origin, WorldPos};
use freeport_core::town::lay;
use freeport_core::walker::Bounds;
use freeport_core::water::Sea;
use std::sync::Arc;

const R: f64 = crate::RADIUS;

/// A town of `radius` at the planet's pole, on its own plan and no
/// planet's ground: what the tiles are cut from.
fn town(radius: f64) -> Town {
    lay(DVec3::Y, 0.0, radius, DVec2::new(1.0, 0.3), 0, 7)
}

/// Every lot and every piece of street is in exactly ONE tile, and a
/// tile is one block: the lots of one block share a tile and its plan
/// bound holds every one of them.
#[test]
fn a_town_is_its_blocks_and_every_part_of_it_is_in_one() {
    let t = town(250.0);
    let tiles = tiles_of(&t, R);
    let mut lots = vec![0; t.lots.len()];
    let mut pieces = vec![0; t.pieces.len()];
    let middle = lot_frame(R, &t, 0.0, 0.0);
    for tile in &tiles {
        let k = tile.lots.first().map(|&i| key(t.lots[i].x, t.lots[i].z));
        for &i in &tile.lots {
            lots[i] += 1;
            let lot = &t.lots[i];
            assert_eq!(Some(key(lot.x, lot.z)), k, "two blocks in one tile");
            let c = middle.local(lot_frame(R, &t, lot.x, lot.z).world(DVec3::ZERO));
            assert!(tile.distance(c) == 0.0, "lot {i} is outside its own tile");
        }
        for &i in &tile.pieces {
            pieces[i] += 1;
        }
    }
    assert!(lots.iter().all(|&n| n == 1), "a lot in no tile or in two");
    assert!(
        pieces.iter().all(|&n| n == 1),
        "a piece in no tile or in two"
    );
    assert!(tiles.len() > 10, "a town of {} tiles", tiles.len());
}

/// The grade a tile is drawn at falls with distance through the three
/// bakes to the block, and a tile swaying on an edge stays where it is.
#[test]
fn a_tiles_grade_follows_distance_with_a_margin() {
    let edges = [80.0, 250.0, 1200.0];
    assert_eq!(grade(MASS, 0.0, edges, 0.15), 0);
    assert_eq!(grade(MASS, 100.0, edges, 0.15), 1);
    assert_eq!(grade(MASS, 600.0, edges, 0.15), 2);
    assert_eq!(grade(0, 5000.0, edges, 0.15), MASS);
    // Either side of the first edge, the grade it was drawn at holds.
    assert_eq!(grade(0, 85.0, edges, 0.15), 0);
    assert_eq!(grade(1, 75.0, edges, 0.15), 1);
    assert_eq!(grade(2, 1300.0, edges, 0.15), 2);
    assert_eq!(grade(MASS, 1100.0, edges, 0.15), MASS);
}

/// Drawn a tile at a time the whole town is what it was drawn whole, at
/// every grade no dearer than the one before, and the BLOCK is a few
/// triangles a building where the nearest bake is thousands.
#[test]
fn the_blocks_of_a_town_are_its_buildings_as_solid_blocks() {
    let t = town(250.0);
    let library = Library::load();
    let tiles = tiles_of(&t, R);
    let mut tris = [0usize; 4];
    for tile in &tiles {
        for (g, n) in tris.iter_mut().enumerate() {
            let drawn = draw(&library, &t, tile, g, R, R - 50.0, 7);
            *n += drawn.triangles;
            if !tile.lots.is_empty() || !tile.pieces.is_empty() {
                assert!(drawn.opaque.is_some() && drawn.aabb.is_some());
            }
            if g == MASS {
                assert!(drawn.glass.is_none(), "a block has glazing");
            }
        }
    }
    let whole = fabric_with(&t, R, |lot| library.model(lot, 0, 7));
    assert_eq!(
        tris[0],
        whole.mesh.triangles(),
        "the tiles are not the town"
    );
    for g in 1..4 {
        assert!(tris[g] <= tris[g - 1], "grade {g} is dearer: {tris:?}");
    }
    let per = tris[MASS] as f64 / (t.lots.len() + t.pieces.len()) as f64;
    assert!(
        per < 12.0,
        "{per:.1} triangles a lot or a piece as blocks: {tris:?}"
    );
    assert!(
        tris[MASS] * 50 < tris[0],
        "the blocks are {} triangles against {} at the nearest bake",
        tris[MASS],
        tris[0]
    );
}

/// What a tile stops a body with is the nearest bake's own boxes and
/// lamps, tile by tile the whole town's, in the world.
#[test]
fn the_boxes_of_a_town_are_its_tiles_boxes() {
    let t = town(250.0);
    let library = Library::load();
    let whole = fabric_with(&t, R, |lot| library.model(lot, 0, 7));
    let (mut blocks, mut lamps) = (0, 0);
    for tile in tiles_of(&t, R) {
        let s = stops(&library, &t, &tile, R, 7);
        for b in &s.blocks {
            let (lo, hi) = b.bounds();
            assert!(lo.cmpge(s.bounds.0).all() && hi.cmple(s.bounds.1).all());
        }
        blocks += s.blocks.len();
        lamps += s.lamps.len();
    }
    assert_eq!(blocks, whole.blocks.len());
    assert_eq!(lamps, whole.lamps.len());
}

/// One town on a bare planet, and an app that streams it.
fn app_with(radius: f64) -> App {
    let t = town(radius);
    let world = World {
        planet: Planet {
            radius: R,
            relief: 0.0,
            overhang: 0.0,
            octaves: 1,
            ..default()
        },
        towns: vec![t],
        roads: vec![],
        routes: vec![],
        bounds: Bounds {
            radius: R,
            floor: R - 2.0,
            top: R + 2.0,
            sea: 0.0,
        },
        sea: Sea { radius: R - 400.0 },
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(Ground(Arc::new(world), DVec3::ZERO))
        .insert_resource(Eye(WorldPos(DVec3::Y * (R + 1.7))))
        .insert_resource(RenderFrame(Origin { at: DVec3::Y * R }))
        .insert_resource(crate::city::stream::Library(Arc::new(Library::default())))
        .insert_resource(Glazing(Handle::default()))
        .insert_resource(Ground3d {
            ground: Handle::default(),
            tarmac: Handle::default(),
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<crate::tuning::Tuning>()
        .init_resource::<Fabric>()
        .init_resource::<Building>()
        .add_systems(Update, (stream_towns, stream_tiles).chain());
    app
}

/// Run an app until its towns settle, and answer how many updates that
/// took, or panic past `most`.
fn settle(app: &mut App, most: usize) -> usize {
    for n in 0..most {
        app.update();
        if app.world().resource::<Fabric>().settled() {
            return n;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the town never settled");
}

/// A town is raised as blocks, the tiles round the eye come in at the
/// nearest bake with their boxes and lamps, the far ones stay blocks,
/// and walking away takes the detail and the boxes back.
#[test]
fn the_tiles_near_the_eye_are_drawn_and_walked_and_the_far_ones_are_blocks() {
    let mut app = app_with(300.0);
    settle(&mut app, 4000);
    let (near, far, stopped, lamps) = {
        let fabric = app.world().resource::<Fabric>();
        assert_eq!(fabric.standing(), vec![0], "the town was not raised");
        let raised = &fabric.towns[0];
        let here = raised.frame.local(DVec3::Y * (R + 1.7));
        let (mut near, mut far, mut stopped, mut lamps) = (0, 0, 0, 0);
        for (tile, state) in raised.tiles.iter().zip(&raised.state) {
            let d = tile.distance(here);
            let shown = state.shown.as_ref().map_or(MASS, |s| s.0);
            if d < 50.0 {
                assert_eq!(shown, 0, "a tile {d:.0} m off is drawn at {shown}");
                near += 1;
            }
            if d > 1500.0 {
                assert_eq!(shown, MASS, "a tile {d:.0} m off is drawn at {shown}");
                far += 1;
            }
            if let Some(s) = &state.stops {
                stopped += 1;
                lamps += s.lamps.len();
                assert!(d < 260.0, "a tile {d:.0} m off carries its boxes");
            } else {
                assert!(d > 180.0, "a tile {d:.0} m off has no boxes");
            }
            assert!(state.mass.is_some() || (tile.lots.is_empty() && tile.pieces.is_empty()));
        }
        (near, far, stopped, lamps)
    };
    assert!(
        near > 0 && stopped > 0 && lamps > 0,
        "{near} near, {stopped} with boxes"
    );
    let _ = far;
    // A body at the eye stands among boxes.
    let fabric = app.world().resource::<Fabric>();
    let ground = app.world().resource::<Ground>();
    let under = fabric.underfoot(&ground.0.planet, DVec3::Y * (R + 1.7), 40.0);
    assert!(!under.blocks.is_empty(), "nothing to walk into in the town");
    // And away over the sea: every tile a block, no boxes held.
    app.world_mut().resource_mut::<Eye>().0 = WorldPos(DVec3::new(0.0, R, 20_000.0));
    for _ in 0..8 {
        app.update();
    }
    let fabric = app.world().resource::<Fabric>();
    let raised = &fabric.towns[0];
    assert!(raised.state.iter().all(|s: &TileState| s.stops.is_none()));
    assert!(raised
        .state
        .iter()
        .all(|s| s.shown.is_none() && s.grade == MASS));
}
