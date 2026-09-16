use super::*;
use crate::{place_eye, walk::toggle_walk, Frame, Ground, World, RADIUS};
use bevy::input::mouse::MouseMotion;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::field::Planet;
use freeport_core::walker::{Bounds, Walker};
use freeport_core::water::Sea;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;
use std::time::Duration;

fn camera() -> Fly {
    Fly::new(DVec3::ZERO, DVec3::NEG_Z, DVec3::Y, 6.0)
}

fn near(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 0.00001,
        "{actual} != {expected}"
    );
}

#[test]
fn pitch_can_loop_through_vertical_and_upside_down() {
    let mut cam = camera();
    for expected in [Vec3::Y, Vec3::Z, Vec3::NEG_Y, Vec3::NEG_Z] {
        for _ in 0..90 {
            cam.turn(Vec2::new(0.0, -PI / 180.0), 0.0);
        }
        near(cam.forward(), expected);
        assert!((cam.rotation.length() - 1.0).abs() < 0.000001);
    }
}

#[test]
fn roll_turns_the_movement_axes_and_mouse_yaw() {
    let mut cam = camera();
    cam.turn(Vec2::ZERO, FRAC_PI_2);
    near(cam.forward(), Vec3::NEG_Z);
    cam.travel(Vec3::Y, 2.0);
    near(cam.at.as_vec3(), Vec3::X * 2.0);
    cam.travel(Vec3::X, 3.0);
    near(cam.at.as_vec3(), Vec3::new(2.0, -3.0, 0.0));
    cam.turn(Vec2::new(FRAC_PI_2, 0.0), 0.0);
    near(cam.forward(), Vec3::NEG_Y);
}

#[test]
fn diagonal_motion_keeps_speed_and_small_steps_keep_world_precision() {
    let mut cam = camera();
    cam.at = DVec3::splat(1_000_000.0);
    let start = cam.at;
    cam.travel(Vec3::new(1.0, 1.0, -1.0), 0.001);
    assert!(((cam.at - start).length() - 0.001).abs() < 1e-9);
}

#[test]
fn level_uses_planet_up_without_changing_the_view_direction() {
    let up = DVec3::new(1.0, -2.0, 3.0).normalize();
    let forward = up.cross(DVec3::X).normalize();
    let mut cam = Fly::new(up * RADIUS, forward, up, 6.0);
    cam.turn(Vec2::new(0.0, -0.4), 1.3);
    let before = cam.forward();
    cam.level(DVec3::ZERO);
    near(cam.forward(), before);
    near(
        cam.rotation * Vec3::X,
        before.cross(up.as_vec3()).normalize(),
    );
    assert!((cam.rotation * Vec3::Y).dot(up.as_vec3()) > 0.9);
    for forward in [up, -up] {
        cam.face(forward, up);
        cam.level(DVec3::ZERO);
        assert!(cam.rotation.is_finite());
        near(cam.forward(), forward.as_vec3());
    }
}

#[test]
fn speed_limits_and_zero_settings_are_safe() {
    let config: FlightSettings = serde_json::from_str(
        r#"{"speed":0,"boost":0,"roll_degrees_per_second":0,"min_speed":0,"max_speed":0,"wheel_factor":0}"#,
    ).unwrap();
    let config = config.validated();
    assert_eq!(config.speed, 6.0);
    let mut cam = camera();
    cam.change_speed(10000.0, &config);
    assert_eq!(cam.speed, config.max_speed);
    cam.change_speed(-10000.0, &config);
    assert_eq!(cam.speed, config.min_speed);
    cam.change_speed(f32::NAN, &config);
    assert_eq!(cam.speed, config.min_speed);
}

fn harness(dt: f64) -> (App, Entity, Entity) {
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f64(dt));
    app.insert_resource(time)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<FlightSettings>()
        .init_resource::<Eye>()
        .init_resource::<Status>()
        .init_resource::<Frame>()
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        .add_systems(Update, (fly, place_eye).chain());
    let window = app
        .world_mut()
        .spawn((
            Window {
                focused: true,
                ..default()
            },
            CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                ..default()
            },
            PrimaryWindow,
        ))
        .id();
    let cam = app.world_mut().spawn((Camera3d::default(), camera())).id();
    (app, cam, window)
}

#[test]
fn roll_keys_are_frame_rate_independent_and_reach_the_rendered_camera() {
    for fps in [30, 120] {
        for (key, up) in [(KeyCode::KeyE, Vec3::X), (KeyCode::KeyQ, Vec3::NEG_X)] {
            let (mut app, entity, _) = harness(1.0 / fps as f64);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            for _ in 0..fps {
                app.update();
            }
            let transform = app.world().get::<Transform>(entity).unwrap();
            near(transform.rotation * Vec3::Y, up);
            near(transform.rotation * Vec3::NEG_Z, Vec3::NEG_Z);
        }
    }
}

#[test]
fn flight_bindings_move_along_camera_axes_and_both_shift_keys_boost() {
    for (key, direction) in [
        (KeyCode::KeyW, Vec3::NEG_Z),
        (KeyCode::KeyS, Vec3::Z),
        (KeyCode::KeyA, Vec3::NEG_Y),
        (KeyCode::KeyD, Vec3::Y),
        (KeyCode::Space, Vec3::NEG_X),
        (KeyCode::ControlLeft, Vec3::X),
        (KeyCode::ControlRight, Vec3::X),
    ] {
        for shift in [None, Some(KeyCode::ShiftLeft), Some(KeyCode::ShiftRight)] {
            let (mut app, entity, _) = harness(0.1);
            app.world_mut()
                .get_mut::<Fly>(entity)
                .unwrap()
                .turn(Vec2::ZERO, -FRAC_PI_2);
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(key);
            if let Some(shift) = shift {
                keys.press(shift);
            }
            app.update();
            let distance = if shift.is_some() { 4.8 } else { 0.6 };
            near(
                app.world().resource::<Eye>().0 .0.as_vec3(),
                direction * distance,
            );
        }
    }
}

#[test]
fn unfocused_window_cannot_move_and_released_mouse_cannot_turn() {
    let (mut app, entity, window) = harness(0.1);
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyE);
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::splat(100.0),
    });
    app.update();
    let cam = app.world().get::<Fly>(entity).unwrap();
    assert_eq!(cam.at, DVec3::ZERO);
    assert_eq!(cam.rotation, Quat::IDENTITY);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    app.world_mut()
        .get_mut::<CursorOptions>(window)
        .unwrap()
        .grab_mode = CursorGrabMode::None;
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::splat(100.0),
    });
    app.update();
    assert_eq!(
        app.world().get::<Fly>(entity).unwrap().rotation,
        Quat::IDENTITY
    );
}

fn ground() -> Ground {
    Ground(
        Arc::new(World {
            planet: Planet {
                radius: RADIUS,
                relief: 0.0,
                overhang: 0.0,
                octaves: 1,
                ..default()
            },
            blocks: vec![],
            groups: vec![],
            lamps: vec![],
            towns: vec![],
            bounds: Bounds {
                radius: RADIUS,
                floor: RADIUS - 2.0,
                top: RADIUS + 2.0,
                sea: 0.0,
            },
            sea: Sea {
                radius: RADIUS - 400.0,
            },
        }),
        DVec3::ZERO,
    )
}

fn walker(ground: &Ground) -> Walker {
    let up = DVec3::new(1.0, -2.0, 3.0).normalize();
    let mut walker = Walker::enter(&ground.0.planet, &ground.0.bounds, up, up.cross(DVec3::X));
    walker.pitch = 0.65;
    walker
}

#[test]
fn toggling_walk_and_fly_preserves_the_view_on_a_planets_side() {
    let (mut app, entity, _) = harness(0.0);
    let ground = ground();
    let walker = walker(&ground);
    let expected = Transform::default().looking_to(walker.look().as_vec3(), walker.dir.as_vec3());
    app.insert_resource(ground)
        .insert_resource(OnFoot(walker.clone()));
    app.add_systems(Update, toggle_walk.before(fly));
    app.world_mut().get_mut::<Fly>(entity).unwrap().speed = 200.0;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    assert!(!app.world().contains_resource::<OnFoot>());
    let cam = app.world().get::<Fly>(entity).unwrap();
    near(cam.forward(), expected.rotation * Vec3::NEG_Z);
    near(cam.rotation * Vec3::Y, expected.rotation * Vec3::Y);
    assert_eq!(cam.at, walker.eye());
    assert_eq!(cam.speed, 200.0);
    // With no input plugin, F remains just_pressed for this second update.
    app.update();
    let returned = &app.world().resource::<OnFoot>().0;
    near(returned.look().as_vec3(), walker.look().as_vec3());
    near(returned.dir.as_vec3(), walker.dir.as_vec3());
}

#[test]
fn wheel_pixels_match_lines_and_on_foot_events_are_not_replayed() {
    for (unit, y) in [
        (MouseScrollUnit::Line, 1.0),
        (MouseScrollUnit::Pixel, 100.0),
    ] {
        let (mut app, entity, window) = harness(0.0);
        let ground = ground();
        app.insert_resource(OnFoot(walker(&ground)));
        app.world_mut().write_message(MouseWheel {
            unit,
            x: 0.0,
            y,
            window,
        });
        app.world_mut().write_message(MouseMotion {
            delta: Vec2::splat(100.0),
        });
        app.update();
        app.world_mut().remove_resource::<OnFoot>();
        app.update();
        let cam = app.world().get::<Fly>(entity).unwrap();
        assert_eq!(cam.speed, 6.0);
        assert_eq!(cam.rotation, Quat::IDENTITY);
        app.world_mut().write_message(MouseWheel {
            unit,
            x: 0.0,
            y,
            window,
        });
        app.update();
        assert!((app.world().get::<Fly>(entity).unwrap().speed - 9.6).abs() < 1e-12);
    }
}

#[test]
fn walking_and_flying_keep_the_global_position_on_a_translated_planet() {
    let (mut app, entity, _) = harness(0.0);
    let mut ground = ground();
    ground.1 = DVec3::new(3.2e6, 8e5, -2e6);
    let walker = walker(&ground);
    let expected = ground.1 + walker.eye();
    app.insert_resource(ground)
        .insert_resource(OnFoot(walker.clone()));
    app.add_systems(Update, toggle_walk.before(fly));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    assert_eq!(app.world().get::<Fly>(entity).unwrap().at, expected);
    assert_eq!(app.world().resource::<Eye>().0 .0, expected);
    app.update();
    let returned = &app.world().resource::<OnFoot>().0;
    assert!((returned.eye() - walker.eye()).length() < 1e-6);
    near(returned.look().as_vec3(), walker.look().as_vec3());
}

#[test]
fn the_flight_system_limits_boost_but_preserves_the_selected_cruise_speed() {
    let (mut app, entity, _) = harness(0.1);
    let ground = ground();
    let centre = DVec3::new(3.2e6, 8e5, -2e6);
    let mut planets = crate::planets::Planets::load(ground.0.clone());
    planets.bodies.truncate(1);
    planets.bodies[0].centre = centre;
    planets.target = 0;
    app.insert_resource(planets);
    {
        let mut cam = app.world_mut().get_mut::<Fly>(entity).unwrap();
        cam.at = centre + DVec3::Y * (RADIUS + 5.0);
        cam.speed = 2e6;
        cam.face(DVec3::NEG_Y, DVec3::Z);
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftRight);
    for _ in 0..30 {
        app.update();
        let cam = app.world().get::<Fly>(entity).unwrap();
        assert!((cam.at - centre).length() >= RADIUS + 0.5 - 1e-8);
        assert_eq!(cam.speed, 2e6);
    }
    let before = app.world().get::<Fly>(entity).unwrap().at;
    assert!((before - centre).length() < RADIUS + 0.52);
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(KeyCode::KeyW);
    keys.press(KeyCode::KeyS);
    app.update();
    assert!(app.world().get::<Fly>(entity).unwrap().at.y > before.y + 0.1);
}

#[test]
fn mouse_look_is_not_scaled_by_frame_time_and_r_restores_the_horizon() {
    for dt in [1.0 / 30.0, 1.0 / 120.0] {
        let (mut app, entity, _) = harness(dt);
        app.world_mut().get_mut::<Fly>(entity).unwrap().at = DVec3::Y * RADIUS;
        app.world_mut().write_message(MouseMotion {
            delta: Vec2::new(80.0, -60.0),
        });
        app.update();
        let cam = app.world().get::<Fly>(entity).unwrap();
        let expected =
            Quat::from_rotation_y(-80.0 * crate::LOOK) * Quat::from_rotation_x(60.0 * crate::LOOK);
        near(cam.forward(), expected * Vec3::NEG_Z);
        app.world_mut()
            .get_mut::<Fly>(entity)
            .unwrap()
            .turn(Vec2::ZERO, 1.2);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyR);
        app.update();
        let cam = app.world().get::<Fly>(entity).unwrap();
        near(cam.forward(), expected * Vec3::NEG_Z);
        near(cam.rotation * Vec3::Y, expected * Vec3::Y);
    }
}

#[test]
fn returning_to_walk_while_looking_radially_up_or_down_has_a_tangent_heading() {
    for up in [DVec3::X, DVec3::new(1.0, -2.0, 3.0).normalize()] {
        for sign in [-1.0, 1.0] {
            let (mut app, entity, _) = harness(0.0);
            app.insert_resource(ground());
            app.add_systems(Update, toggle_walk.before(fly));
            *app.world_mut().get_mut::<Fly>(entity).unwrap() =
                Fly::new(up * (RADIUS + 1.7), up * sign, up, 6.0);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyF);
            app.update();
            let walker = &app.world().resource::<OnFoot>().0;
            assert!(walker.fwd.is_finite());
            assert!(walker.fwd.dot(walker.dir).abs() < 1e-8);
            assert!((walker.fwd.length() - 1.0).abs() < 1e-8);
            assert!((walker.pitch - sign * 1.45).abs() < 1e-8);
        }
    }
}
