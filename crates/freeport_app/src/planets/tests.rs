use super::*;

fn system(centre: DVec3) -> Planets {
    let mut body = Body::from_definition(Definition {
        name: "test".into(),
        centre: centre.to_array(),
        radius: 1000.0,
        relief: 0.0,
        sea_offset: -100.0,
        seed: 7,
        colour: [1.0; 3],
    })
    .unwrap();
    Arc::make_mut(&mut body.world).planet.overhang = 0.0;
    Planets {
        bodies: vec![body],
        ..default()
    }
}

#[test]
fn boosted_space_approach_cannot_skip_air_or_ground_at_any_frame_rate() {
    for fps in [10, 30, 60, 144] {
        let centre = DVec3::new(3.2e6, 8e5, -2e6);
        let planets = system(centre);
        let mut at = centre + DVec3::Y * 3000.0;
        let mut slowed = false;
        for _ in 0..fps * 14 {
            let (next, speed) = planets.advance(at, DVec3::NEG_Y, 16e6, 6.0, 0.5, 1.0 / fps as f64);
            let height = (next - centre).length() - 1000.0;
            assert!(height >= 0.5 - 1e-8, "{fps} fps: {height}");
            slowed |= speed < 100.0;
            at = next;
        }
        assert!(slowed);
        assert!(((at - centre).length() - 1000.5).abs() < 0.02);
    }
}

#[test]
fn a_ground_contact_can_take_off_again_and_invalid_starts_recover() {
    let planets = system(DVec3::ZERO);
    let (recovered, _) = planets.advance(DVec3::ZERO, DVec3::ZERO, 2e6, 6.0, 0.5, 0.1);
    assert!(recovered.y >= 1000.5);
    let (landed, _) = planets.advance(recovered, DVec3::NEG_Y, 2e6, 6.0, 0.5, 0.1);
    let (departed, _) = planets.advance(landed, DVec3::Y, 2e6, 6.0, 0.5, 0.1);
    assert!(departed.y > landed.y + 0.1, "stuck at {landed}, {departed}");
}

#[test]
fn space_has_full_cruise_speed_and_a_missed_planet_does_not_slow_it() {
    let planets = system(DVec3::ZERO);
    let from = DVec3::Y * 3000.0;
    let (to, speed) = planets.advance(from, DVec3::X, 2e6, 6.0, 0.5, 0.1);
    assert_eq!(speed, 2e6);
    assert_eq!(to, from + DVec3::X * 200_000.0);
}

#[test]
fn planet_relative_motion_is_translation_invariant_at_interplanetary_distances() {
    let offset = DVec3::new(3.2e9, -1e9, 8e9);
    let a = system(DVec3::ZERO);
    let b = system(offset);
    let start = DVec3::Y * 1010.0;
    let direction = DVec3::new(0.5, -1.0, 0.0).normalize();
    let (left, _) = a.advance(start, direction, 2e6, 6.0, 0.5, 0.1);
    let (right, _) = b.advance(offset + start, direction, 2e6, 6.0, 0.5, 0.1);
    assert!((left - (right - offset)).length() < 1e-4);
}

#[test]
fn nearby_bodies_have_separate_fields_and_local_atmospheres() {
    let home = system(DVec3::ZERO).bodies.remove(0).world;
    let planets = Planets::load(home);
    assert_eq!(planets.bodies.len(), 4);
    for (i, body) in planets.bodies.iter().enumerate() {
        assert_eq!(planets.nearest(body.centre + DVec3::Y * body.air.top), i);
        assert!(body.world.planet.at(DVec3::ZERO) > 0.0);
    }
}

#[test]
fn changing_planets_updates_ground_streamer_and_weather_together() {
    use freeport_core::lattice::Lattice;
    use freeport_core::pos::WorldPos;
    let mut planets = system(DVec3::ZERO);
    let centre = DVec3::new(5000.0, 0.0, 0.0);
    planets.bodies.push(system(centre).bodies.remove(0));
    let expected = planets.bodies[1].world.clone();
    let home = planets.bodies[0].world.clone();
    let weather = crate::sky::Weather {
        air: planets.bodies[0].air,
        sea: 900.0,
        sun: DVec3::Z,
    };
    let mut app = App::new();
    app.insert_resource(planets)
        .insert_resource(Ground(home, DVec3::ZERO))
        .insert_resource(weather)
        .insert_resource(crate::Eye(WorldPos(centre + DVec3::Y * 1100.0)))
        .insert_resource(crate::stream::Streamer::new(
            Lattice::new(DVec3::ZERO, 0.25),
            DVec3::Y * 1100.0,
            4,
            default(),
            default(),
            None,
            crate::tuning::Tuning {
                terrain_workers: 1,
                ..default()
            },
        ))
        .add_systems(Update, activate);
    app.update();
    assert_eq!(app.world().resource::<Planets>().active, 1);
    assert_eq!(app.world().resource::<Ground>().1, centre);
    assert!(Arc::ptr_eq(&app.world().resource::<Ground>().0, &expected));
    assert_eq!(
        app.world().resource::<crate::stream::Streamer>().centre,
        centre
    );
    assert_eq!(app.world().resource::<crate::sky::Weather>().sun, DVec3::Z);
    assert_eq!(
        app.world().resource::<crate::sky::Weather>().sea,
        expected.sea.radius
    );
}
