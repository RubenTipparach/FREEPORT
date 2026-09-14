//! freeport_app: the Bevy harness. It draws what `freeport_core` says.
//!
//! A planetoid dual contoured on the two level lattice, with a pad, a wall
//! and a step built on its top on the fine level, wearing the baked sets
//! (`terrain.rs`), with the core's walker on it (`walk.rs`) and a fly
//! camera for looking at the join, a wireframe toggle, and a screenshot
//! flag so a picture can be taken headless under Xvfb and lavapipe. The
//! floating origin, the streamer and the ship arrive in the stages
//! `CLAUDE.md` lays out, each behind its own mockup.
//!
//! ```text
//! freeport_app [--sub N] [--wire] [--fly] [--eye x,y,z] [--look x,y,z]
//!              [--shot out.png] [--frames N]
//! ```
//!
//! Left click takes the mouse, Escape gives it back. On foot: WASD, Shift
//! runs, Space jumps. Flying: WASD and Q E, Shift is faster. F swaps the
//! two, Tab toggles the wireframe. `--eye` is where to start (on foot, the
//! spot under it) and `--look` what to face.

mod terrain;
mod walk;

use bevy::camera::Exposure;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::MouseMotion;
use bevy::math::DVec3;
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::audit::{audit, Audit};
use freeport_core::dc::{contour, Coarse, DcMesh};
use freeport_core::field::{Block, Built, Density, Planet};
use freeport_core::lattice::{Lattice, CH};
use freeport_core::walker::{Bounds, Walker};
use std::time::Instant;
use terrain::{terrain_material, to_mesh, TerrainMaterial, TerrainPlugin};
use walk::{place_camera, toggle_walk, walk, OnFoot, Stat};

/// The planetoid the harness shows: its radius and the lattice it is
/// contoured on. A metre a cell over 96 cells holds a 40 m ball with room
/// for its relief.
const RADIUS: f64 = 40.0;
const CELLS: usize = 96;
const CELL: f64 = 1.0;
/// How far round the site the ground is on the fine level, metres.
const SITE_REACH: f64 = 6.0;
/// What is built snaps to this, metres, and the lattice's corner sits half
/// a fine cell off it, so no face of a block ever lies on a lattice plane:
/// a face that does puts its crease on a lattice edge and the cells either
/// side of it solve to one point, which the core's audit counts as a pinch.
const SNAP: f64 = 0.5;
/// Radians of look per pixel of mouse.
pub(crate) const LOOK: f32 = 0.0022;
/// Flying: metres a second, and the factor Shift puts on it.
const SPEED: f32 = 4.0;
const SPRINT: f32 = 4.0;

/// What the command line asked for.
#[derive(Resource, Clone, Debug)]
struct Args {
    sub: usize,
    wire: bool,
    fly: bool,
    eye: Option<Vec3>,
    look: Option<Vec3>,
    shot: Option<String>,
    frames: u32,
}

fn parse_args() -> Args {
    let mut args = Args {
        sub: 4,
        wire: false,
        fly: false,
        eye: None,
        look: None,
        shot: None,
        frames: 30,
    };
    let mut it = std::env::args().skip(1);
    let vec3 = |s: &str| -> Option<Vec3> {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sub" => {
                args.sub = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(4)
                    .clamp(1, 8)
            }
            "--wire" => args.wire = true,
            "--fly" => args.fly = true,
            "--eye" => args.eye = it.next().and_then(|v| vec3(&v)),
            "--look" => args.look = it.next().and_then(|v| vec3(&v)),
            "--shot" => args.shot = it.next(),
            "--frames" => args.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(30),
            other => warn!("unknown argument {other}"),
        }
    }
    args
}

fn main() {
    let args = parse_args();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((WireframePlugin::default(), TerrainPlugin))
        .insert_resource(WireframeConfig {
            global: args.wire,
            default_color: Color::srgb(0.1, 0.1, 0.12),
        })
        .insert_resource(args)
        .add_systems(Startup, spawn_world)
        .add_systems(
            Update,
            (
                grab_mouse,
                toggle_walk,
                walk,
                fly,
                toggle_wireframe,
                take_shot,
            )
                .chain(),
        )
        .run();
}

/// What a frame of input is read from: the clock, the keys, the mouse's
/// motion and whether the window holds it. One thing, so a system that
/// reads the player is not a system with eight arguments.
#[derive(SystemParam)]
pub(crate) struct Controls<'w, 's> {
    pub time: Res<'w, Time>,
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    motion: MessageReader<'w, 's, MouseMotion>,
    cursor: Query<'w, 's, &'static CursorOptions, With<PrimaryWindow>>,
}

impl Controls<'_, '_> {
    /// This frame's look, radians about the local up and of tilt, from the
    /// mouse while the window holds it. One event carrying a whole screen
    /// is a window handing focus back, never a look; it is clamped rather
    /// than turned twice round.
    pub fn look(&mut self) -> Vec2 {
        let taken = self
            .cursor
            .single()
            .map(|c| c.grab_mode == CursorGrabMode::Locked)
            .unwrap_or(false);
        let mut look = Vec2::ZERO;
        for m in self.motion.read() {
            if taken {
                look += m.delta.clamp(Vec2::splat(-200.0), Vec2::splat(200.0)) * LOOK;
            }
        }
        look
    }
}

/// The fly camera's heading, kept as angles so a look cannot roll.
#[derive(Component)]
pub(crate) struct Fly {
    pub yaw: f32,
    pub pitch: f32,
}

/// The field the harness stands on: the planet, what is built on it, and
/// where the walker may look for the ground.
#[derive(Resource)]
pub(crate) struct Ground {
    pub planet: Planet,
    pub blocks: Vec<Block>,
    pub bounds: Bounds,
}

impl Ground {
    /// The same field the mesher contoured.
    pub fn field(&self) -> Built<'_> {
        Built {
            ground: &self.planet,
            blocks: self.blocks.clone(),
        }
    }
}

/// The planet, and the pad, the wall and the step on its top.
fn world() -> Ground {
    let planet = Planet {
        radius: RADIUS,
        relief: 6.0,
        lumps: 3.0,
        octaves: 5,
        overhang: 1.5,
        ledge: 4.0,
        seed: 7,
    };
    let top = (top_of(&planet) / SNAP).round() * SNAP;
    let frame = [DVec3::X, DVec3::Z, DVec3::Y];
    let blocks = vec![
        // A pad half a metre over the ground at the site's middle and dug
        // into the rise beside it, a wall on it, and a step.
        Block {
            centre: DVec3::new(0.0, top - 0.5, 0.0),
            half: DVec3::new(3.0, 3.0, 1.0),
            axes: frame,
        },
        Block {
            centre: DVec3::new(2.0, top + 1.6, 0.0),
            half: DVec3::new(0.2, 2.5, 1.1),
            axes: frame,
        },
        Block {
            centre: DVec3::new(-1.5, top + 0.7, -2.0),
            half: DVec3::new(0.6, 0.6, 0.2),
            axes: frame,
        },
    ];
    let bounds = Bounds {
        radius: RADIUS,
        floor: RADIUS - planet.relief * 0.6 - planet.overhang - 2.0,
        top: RADIUS + planet.relief * 0.6 + planet.overhang + 6.0,
    };
    Ground {
        planet,
        blocks,
        bounds,
    }
}

/// The height of the ground along +y, by bisection on the field.
fn top_of(planet: &Planet) -> f64 {
    let (mut lo, mut hi) = (RADIUS - 6.0, RADIUS + 6.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if planet.at(DVec3::new(0.0, mid, 0.0)) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Contour every chunk of the lattice, with the field's audit.
fn build(field: &dyn Density, lat: &Lattice) -> (Vec<(DVec3, DcMesh)>, Audit) {
    let t0 = Instant::now();
    let coarse = Coarse::sample(field, lat);
    let sampled = t0.elapsed();
    let cn = lat.chunks();
    let mut chunks = Vec::new();
    for bz in 0..cn {
        for by in 0..cn {
            for bx in 0..cn {
                let m = contour(field, lat, &coarse, [bx, by, bz]);
                if m.triangles() > 0 {
                    let span = (CH * lat.sub) as i64;
                    let corner = lat.point([bx as i64 * span, by as i64 * span, bz as i64 * span]);
                    chunks.push((corner, m));
                }
            }
        }
    }
    let contoured = t0.elapsed();
    let a = audit(field, &chunks);
    info!(
        "contoured {} chunks, {} triangles ({} seam polygons) in {:.0} ms ({:.0} ms of coarse samples), audited in {:.0} ms: {} open edges, {} pinches, {} facing in, {} missing corners, area {:.0} m2",
        chunks.len(),
        a.triangles,
        a.seams,
        contoured.as_secs_f64() * 1000.0,
        sampled.as_secs_f64() * 1000.0,
        (t0.elapsed() - contoured).as_secs_f64() * 1000.0,
        a.open,
        a.non_manifold,
        a.facing_in,
        a.missing,
        a.area
    );
    (chunks, a)
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    args: Res<Args>,
) {
    let ground = world();
    let top = ground.blocks[0].centre.y + 0.5;
    let built = ground.field();
    let half = CELLS as f64 * CELL * 0.5;
    let offset = 0.5 * CELL / args.sub as f64;
    let mut lat = Lattice::new(DVec3::splat(-half + offset), CELL, args.sub, CELLS);
    let site = DVec3::new(0.0, top, 0.0);
    let masked = lat.subdivide_near(site, SITE_REACH);
    let grown = lat.grow(&built);
    info!(
        "lattice: {}^3 cells of {} m, {} fine under the site at y = {:.2} and {} grown, {} m fine cells",
        CELLS, CELL, masked, top, grown, lat.fine
    );
    let (chunks, _) = build(&built, &lat);
    let material = terrain_material(&mut images, &mut materials);
    for (corner, m) in &chunks {
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(m))),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(corner.as_vec3()),
        ));
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 60.0,
        ..default()
    });
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.9, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            bottom: Val::Px(10.0),
            ..default()
        },
        Stat,
    ));
    spawn_camera(&mut commands, &ground, &args, top);
    commands.insert_resource(ground);
}

/// The camera, flying from `--eye` toward `--look`, or on foot at the spot
/// under `--eye` facing `--look`, by default at the site's south edge
/// looking north at the pad.
fn spawn_camera(commands: &mut Commands, ground: &Ground, args: &Args, top: f64) {
    let eye = args.eye.unwrap_or(Vec3::new(0.5, top as f32, -6.5));
    let look = args.look.unwrap_or(Vec3::new(0.0, top as f32 + 0.8, 0.0));
    let d = (look - eye).normalize_or(Vec3::NEG_Z);
    let fly = Fly {
        yaw: (-d.x).atan2(-d.z),
        pitch: d.y.clamp(-1.0, 1.0).asin(),
    };
    let mut tf = Transform::from_translation(eye).with_rotation(Quat::from_euler(
        EulerRot::YXZ,
        fly.yaw,
        fly.pitch,
        0.0,
    ));
    if !args.fly {
        let w = Walker::enter(
            &ground.field(),
            &ground.bounds,
            eye.as_dvec3(),
            d.as_dvec3(),
        );
        place_camera(&w, &mut tf);
        commands.insert_resource(OnFoot(w));
    }
    commands.spawn((Camera3d::default(), Exposure { ev100: 10.5 }, tf, fly));
}

/// Left click takes the mouse, Escape gives it back.
fn grab_mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor.single_mut() else {
        return;
    };
    if buttons.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Fly the camera while nobody is on foot: the mouse turns it while it is
/// taken, and the keys move it in its own frame.
fn fly(
    mut controls: Controls,
    on_foot: Option<Res<OnFoot>>,
    mut cam: Query<(&mut Transform, &mut Fly)>,
) {
    let look = controls.look();
    if on_foot.is_some() {
        return;
    }
    let Ok((mut tf, mut fly)) = cam.single_mut() else {
        return;
    };
    fly.yaw -= look.x;
    fly.pitch = (fly.pitch - look.y).clamp(-1.5, 1.5);
    tf.rotation = Quat::from_euler(EulerRot::YXZ, fly.yaw, fly.pitch, 0.0);
    let keys = &controls.keys;
    let mut v = Vec3::ZERO;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
    v += tf.forward().as_vec3() * axis(KeyCode::KeyS, KeyCode::KeyW);
    v += tf.right().as_vec3() * axis(KeyCode::KeyA, KeyCode::KeyD);
    v += Vec3::Y * axis(KeyCode::KeyQ, KeyCode::KeyE);
    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        SPEED * SPRINT
    } else {
        SPEED
    };
    let step = v.normalize_or_zero() * speed * controls.time.delta_secs().min(0.1);
    if step.is_finite() {
        tf.translation += step;
    }
}

fn toggle_wireframe(keys: Res<ButtonInput<KeyCode>>, mut config: ResMut<WireframeConfig>) {
    if keys.just_pressed(KeyCode::Tab) {
        config.global = !config.global;
    }
}

/// With `--shot`, save the frame the arguments asked for and leave a few
/// frames later, once the write has had its chance.
fn take_shot(
    mut commands: Commands,
    args: Res<Args>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &args.shot else {
        return;
    };
    *frame += 1;
    if *frame == args.frames {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    if *frame == args.frames + 12 {
        exit.write(AppExit::Success);
    }
}
