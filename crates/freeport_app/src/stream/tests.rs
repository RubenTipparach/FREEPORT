use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use std::sync::mpsc::channel;

fn harness() -> (App, Sender<Done>) {
    let (jobs, _) = channel();
    let (send, done) = channel();
    let streamer = Streamer {
        centre: DVec3::ZERO,
        epoch: 0,
        lat: Lattice::new(DVec3::ZERO, 0.25),
        rings: Rings::around(&Lattice::new(DVec3::ZERO, 0.25), DVec3::ZERO, 4),
        wanted: HashMap::new(),
        loaded: HashMap::new(),
        staged: HashMap::new(),
        building: true,
        has_layout: true,
        planner: Planner::new(),
        planning: false,
        todo: VecDeque::new(),
        remaining: 0,
        retired: VecDeque::new(),
        pending: HashMap::new(),
        jobs,
        done: Mutex::new(done),
        workers: 1,
        tuning: crate::tuning::Tuning {
            terrain_upload_count: 1,
            ..default()
        },
        material: default(),
        water: default(),
        fresh: false,
        stats: default(),
        started: Instant::now(),
        settled: None,
        timings: Timings::default(),
    };
    let mut app = App::new();
    app.insert_resource(streamer)
        .init_resource::<Assets<Mesh>>()
        .add_systems(
            Update,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut stream: ResMut<Streamer>| {
                stream.retire(&mut commands);
                let swell = crate::water::swell_bound(&crate::water::sheet_ext(999600.0));
                stream.drain(&mut commands, &mut meshes, &Origin::default(), swell);
                stream.publish(&mut commands);
            },
        );
    (app, send)
}

fn old_chunk(app: &mut App, id: ChunkId) -> Entity {
    let entity = app.world_mut().spawn(Visibility::Inherited).id();
    app.world_mut()
        .resource_mut::<Streamer>()
        .loaded
        .insert(id, (vec![entity], 1, 1));
    app.world_mut().resource_mut::<Streamer>().stats.triangles += 1;
    entity
}

fn request(app: &mut App, id: ChunkId, sig: u64) {
    let mut stream = app.world_mut().resource_mut::<Streamer>();
    if stream.wanted.insert(id, sig) != Some(sig) {
        stream.remaining += 1;
    }
    stream
        .pending
        .insert(id, (sig, Arc::new(AtomicBool::new(false))));
}

fn result(id: ChunkId, sig: u64, empty: bool) -> Done {
    Done {
        epoch: 0,
        id,
        sig,
        triangles: usize::from(!empty),
        ms: 1.0,
        mesh: (!empty).then(|| {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
        }),
        sheet: None,
    }
}

#[test]
fn a_late_result_from_another_planet_cannot_erase_its_replacements_pending_job() {
    let (mut app, send) = harness();
    let id = ChunkId {
        level: 0,
        at: [0; 3],
    };
    {
        let mut streamer = app.world_mut().resource_mut::<Streamer>();
        streamer.epoch = 1;
        streamer.centre = DVec3::new(3.2e6, 8e5, -2e6);
    }
    request(&mut app, id, 1);
    send.send(result(id, 1, false)).unwrap();
    app.update();
    let streamer = app.world().resource::<Streamer>();
    assert!(streamer.pending.contains_key(&id));
    assert!(streamer.loaded.is_empty());
    let mut current = result(id, 1, false);
    current.epoch = 1;
    send.send(current).unwrap();
    app.update();
    let streamer = app.world().resource::<Streamer>();
    assert!(streamer.idle());
    let entity = streamer.loaded[&id].0[0];
    assert_eq!(
        app.world().get::<Anchored>(entity).unwrap().at.0,
        streamer.centre
    );
}

#[test]
fn lod_replacements_and_adjacent_seams_publish_together_in_both_directions() {
    for (old_level, new_level) in [(1, 0), (0, 3)] {
        let (mut app, send) = harness();
        let old = ChunkId {
            level: old_level,
            at: [0; 3],
        };
        let new = ChunkId {
            level: new_level,
            at: [0; 3],
        };
        let neighbor = ChunkId {
            level: old_level,
            at: [-1, 0, 0],
        };
        let old_entity = old_chunk(&mut app, old);
        let neighbor_entity = old_chunk(&mut app, neighbor);
        request(&mut app, new, 12);
        request(&mut app, neighbor, 18);
        let mut replacement = result(new, 12, false);
        replacement.triangles = 7;
        send.send(replacement).unwrap();
        app.update();
        let staged_entity = app.world().resource::<Streamer>().staged[&new].0[0];
        assert_eq!(
            app.world().get::<Visibility>(staged_entity),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(old_entity),
            Some(&Visibility::Inherited)
        );
        assert_eq!(
            app.world().get::<Visibility>(neighbor_entity),
            Some(&Visibility::Inherited)
        );
        assert!(!app.world().resource::<Streamer>().idle());
        assert_eq!(app.world().resource::<Streamer>().stats.triangles, 2);
        // Spatial coverage arrived, but the adjoining chunk's old seam must
        // remain intact until its own replacement has reached the upload queue.
        send.send(result(neighbor, 18, false)).unwrap();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(old_entity),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(neighbor_entity),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(staged_entity),
            Some(&Visibility::Inherited)
        );
        let stream = app.world().resource::<Streamer>();
        assert!(stream.idle());
        assert!(stream.staged.is_empty());
        assert_eq!(stream.loaded[&neighbor].1, 18);
        assert_eq!(stream.stats.triangles, 8);
        app.update();
        app.update();
        assert!(app.world().get_entity(old_entity).is_err());
        assert!(app.world().get_entity(neighbor_entity).is_err());
    }
}

#[test]
fn empty_replacements_are_required_and_stale_results_cannot_publish() {
    let (mut app, send) = harness();
    let id = ChunkId {
        level: 1,
        at: [0; 3],
    };
    let old = old_chunk(&mut app, id);
    request(&mut app, id, 10);
    send.send(result(id, 9, true)).unwrap();
    app.update();
    assert!(!app.world().resource::<Streamer>().idle());
    assert!(app.world().get_entity(old).is_ok());
    request(&mut app, id, 10);
    send.send(result(id, 10, true)).unwrap();
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Hidden)
    );
    assert!(app.world().resource::<Streamer>().idle());
    assert!(app.world().resource::<Streamer>().loaded[&id].0.is_empty());
    app.update();
    assert!(app.world().get_entity(old).is_err());
}

#[test]
fn initial_chunks_draw_progressively_before_the_first_complete_layout() {
    let (mut app, send) = harness();
    app.world_mut().resource_mut::<Streamer>().has_layout = false;
    let a = ChunkId {
        level: 0,
        at: [0; 3],
    };
    let b = ChunkId {
        level: 0,
        at: [1, 0, 0],
    };
    request(&mut app, a, 1);
    request(&mut app, b, 1);
    send.send(result(a, 1, false)).unwrap();
    app.update();
    let entity = app.world().resource::<Streamer>().loaded[&a].0[0];
    assert_eq!(
        app.world().get::<Visibility>(entity),
        Some(&Visibility::Inherited)
    );
    assert!(!app.world().resource::<Streamer>().idle());
    send.send(result(b, 1, true)).unwrap();
    app.update();
    assert!(app.world().resource::<Streamer>().idle());
}

#[test]
fn streamed_meshes_keep_local_bounds_and_normal_frustum_culling() {
    let (mut app, send) = harness();
    let id = ChunkId {
        level: 1,
        at: [2, 3, -1],
    };
    request(&mut app, id, 1);
    let mut done = result(id, 1, false);
    let points = vec![[-2.0, 1.0, 0.0], [3.0, 4.0, 0.0], [0.0, 0.0, 5.0]];
    done.mesh
        .as_mut()
        .unwrap()
        .insert_attribute(Mesh::ATTRIBUTE_POSITION, points);
    done.sheet = done.mesh.clone();
    send.send(done).unwrap();
    app.update();
    let stream = app.world().resource::<Streamer>();
    for &entity in &stream.loaded[&id].0 {
        let bounds = app
            .world()
            .get::<bevy::camera::primitives::Aabb>(entity)
            .unwrap();
        assert_eq!(Vec3::from(bounds.center), Vec3::new(0.5, 2.0, 2.5));
        let padding = if app.world().get::<Sheet>(entity).is_some() {
            crate::water::swell_bound(&crate::water::sheet_ext(999600.0))
        } else {
            0.0
        };
        assert_eq!(
            Vec3::from(bounds.half_extents),
            Vec3::new(2.5, 2.0, 2.5) + Vec3::splat(padding)
        );
        assert!(app.world().get::<NoAutoAabb>(entity).is_some());
        assert!(app
            .world()
            .get::<bevy::camera::visibility::NoFrustumCulling>(entity)
            .is_none());
        assert_eq!(
            app.world().get::<Anchored>(entity).unwrap().at.0,
            id.corner(&stream.lat)
        );
    }
}

#[test]
fn moving_waits_for_the_current_layout_then_catches_up() {
    let (mut app, _) = harness();
    let world = test_world();
    let mut stream = app.world_mut().resource_mut::<Streamer>();
    let before = stream.rings.clone();
    let eye = DVec3::new(1200.0, 1e6, 200.0);
    stream.want(eye, &world);
    assert_eq!(stream.rings, before);
    stream.building = false;
    stream.want(eye, &world);
    assert_ne!(stream.rings, before);
    assert!(stream.building && stream.planning);
}

fn test_world() -> Arc<World> {
    Arc::new(World {
        planet: freeport_core::field::Planet::default(),
        towns: vec![],
        roads: vec![],
        bounds: freeport_core::walker::Bounds {
            radius: 1e6,
            floor: 990000.0,
            top: 1010000.0,
            sea: 0.0,
        },
        sea: freeport_core::water::Sea { radius: 999600.0 },
    })
}

#[test]
fn old_layout_stays_visible_until_background_planning_finishes() {
    let (mut app, _) = harness();
    let old = old_chunk(
        &mut app,
        ChunkId {
            level: 0,
            at: [0; 3],
        },
    );
    app.world_mut().resource_mut::<Streamer>().planning = true;
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert!(!app.world().resource::<Streamer>().idle());
    app.world_mut()
        .resource_mut::<Streamer>()
        .begin_layout(Layout {
            epoch: 0,
            wanted: HashMap::new(),
            nearest: vec![],
            ms: 0.0,
        });
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Hidden)
    );
    assert!(app.world().resource::<Streamer>().idle());
}

#[test]
fn prepared_queue_reuses_exact_seams_and_prioritizes_new_coverage() {
    let (mut app, _) = harness();
    let ids: Vec<_> = (0..4)
        .map(|x| ChunkId {
            level: 0,
            at: [x, 0, 0],
        })
        .collect();
    old_chunk(&mut app, ids[0]);
    old_chunk(&mut app, ids[1]);
    let mut stream = app.world_mut().resource_mut::<Streamer>();
    stream.begin_layout(Layout {
        epoch: 0,
        wanted: ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, if i == 1 { 2 } else { 1 }))
            .collect(),
        nearest: ids.clone(),
        ms: 0.0,
    });
    assert_eq!(stream.todo, [(ids[2], 1), (ids[3], 1), (ids[1], 2)]);
    assert_eq!(stream.remaining, 3);
    assert_eq!(stream.stats.wanted, 4);
}

#[test]
fn retirement_is_budgeted_without_leaving_old_geometry_visible() {
    let (mut app, _) = harness();
    let old: Vec<_> = (0..4)
        .map(|x| {
            old_chunk(
                &mut app,
                ChunkId {
                    level: 0,
                    at: [x, 0, 0],
                },
            )
        })
        .collect();
    app.update();
    for &entity in &old {
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden)
        );
    }
    assert_eq!(app.world().resource::<Streamer>().stats.triangles, 0);
    for left in [2, 0] {
        app.update();
        assert_eq!(
            old.iter()
                .filter(|&&entity| app.world().get_entity(entity).is_ok())
                .count(),
            left
        );
    }
}

#[test]
#[ignore = "release-mode CPU timing, run with --ignored --nocapture"]
fn moving_layout_planning_cost() {
    let mut world = test_world();
    let planet = &mut Arc::make_mut(&mut world).planet;
    planet.octaves = 18;
    planet.overhang = 3.0;
    planet.ledge = 12.0;
    let lat = Lattice::new(DVec3::splat(-2000100.0), 0.5);
    let eye = DVec3::Y * (freeport_core::town::surface_radius(planet, DVec3::Y) + 12.0);
    let planner = Planner::new();
    let mut synchronous_ms = 0.0;
    let mut submitted_ms = 0.0;
    let mut count = 0;
    for step in 0..30 {
        let eye = eye + DVec3::X * (step as f64 * 16.0);
        let request = || Request {
            epoch: 0,
            lat,
            rings: Rings::around(&lat, eye, 11),
            eye,
            world: world.clone(),
        };
        let start = Instant::now();
        let direct = request().build();
        synchronous_ms += start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        assert!(planner.request(request()));
        submitted_ms += start.elapsed().as_secs_f64() * 1000.0;
        let timeout = Instant::now();
        let planned = loop {
            if let Some(layout) = planner.poll() {
                break layout;
            }
            assert!(timeout.elapsed().as_secs() < 10);
            std::thread::yield_now();
        };
        assert_eq!(planned.wanted, direct.wanted);
        assert_eq!(planned.nearest, direct.nearest);
        count += direct.wanted.len();
    }
    eprintln!("30 layouts, mean {} chunks: synchronous planning {:.3} ms/layout; main-thread submission {:.3} ms/layout", count / 30, synchronous_ms / 30.0, submitted_ms / 30.0);
}
