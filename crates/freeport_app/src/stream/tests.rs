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
    };
    let mut app = App::new();
    app.insert_resource(streamer)
        .init_resource::<Assets<Mesh>>()
        .add_systems(
            Update,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut stream: ResMut<Streamer>| {
                stream.drain(&mut commands, &mut meshes, &Origin::default());
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
    entity
}

fn request(app: &mut App, id: ChunkId, sig: u64) {
    let mut stream = app.world_mut().resource_mut::<Streamer>();
    stream.wanted.insert(id, sig);
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
        send.send(result(new, 12, false)).unwrap();
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
        // Spatial coverage arrived, but the adjoining chunk's old seam must
        // remain intact until its own replacement has reached the upload queue.
        send.send(result(neighbor, 18, false)).unwrap();
        app.update();
        assert!(app.world().get_entity(old_entity).is_err());
        assert!(app.world().get_entity(neighbor_entity).is_err());
        assert_eq!(
            app.world().get::<Visibility>(staged_entity),
            Some(&Visibility::Inherited)
        );
        let stream = app.world().resource::<Streamer>();
        assert!(stream.idle());
        assert!(stream.staged.is_empty());
        assert_eq!(stream.loaded[&neighbor].1, 18);
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
    assert!(app.world().get_entity(old).is_err());
    assert!(app.world().resource::<Streamer>().idle());
    assert!(app.world().resource::<Streamer>().loaded[&id].0.is_empty());
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
fn moving_waits_for_the_current_layout_then_catches_up() {
    let (mut app, _) = harness();
    let world = World {
        planet: freeport_core::field::Planet::default(),
        blocks: vec![],
        groups: vec![],
        lamps: vec![],
        towns: vec![],
        bounds: freeport_core::walker::Bounds {
            radius: 1e6,
            floor: 990000.0,
            top: 1010000.0,
            sea: 0.0,
        },
        sea: freeport_core::water::Sea { radius: 999600.0 },
    };
    let mut stream = app.world_mut().resource_mut::<Streamer>();
    let before = stream.rings.clone();
    let eye = DVec3::new(1200.0, 1e6, 200.0);
    stream.want(eye, &world);
    assert_eq!(stream.rings, before);
    stream.building = false;
    stream.want(eye, &world);
    assert_ne!(stream.rings, before);
    assert!(stream.building);
}
