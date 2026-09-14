//! freeport_app: the Bevy harness. It draws what `freeport_core` says.
//!
//! A planetoid dual contoured on the two level lattice, with a slab and a
//! wall built on its top on the fine level, drawn a chunk a mesh with the
//! coarse and the fine vertices in two colours so the seam between them can
//! be looked at. A fly camera, a wireframe toggle, and a screenshot flag so
//! the picture can be taken headless under Xvfb and lavapipe, which is how
//! the join was checked here before anybody flew round it. The floating
//! origin, the streamer, the ship and the walker arrive in the stages
//! `CLAUDE.md` lays out, each behind its own mockup.
//!
//! ```text
//! freeport_app [--sub N] [--wire] [--eye x,y,z] [--look x,y,z]
//!              [--shot out.png] [--frames N]
//! ```
//!
//! On foot: left click takes the mouse, Escape gives it back, WASD and Q E
//! fly, Shift is faster, Tab toggles the wireframe.

use bevy::asset::RenderAssetUsages;
use bevy::camera::Exposure;
use bevy::input::mouse::MouseMotion;
use bevy::math::DVec3;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::audit::{audit, Audit};
use freeport_core::dc::{contour, Coarse, DcMesh};
use freeport_core::field::{Block, Built, Density, Planet};
use freeport_core::lattice::{Lattice, CH};
use std::time::Instant;

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

/// The coarse and the fine level's vertex colours, so a seam polygon, whose
/// corners are cells of both sizes, blends the two.
const COARSE_COLOUR: [f32; 4] = [0.70, 0.60, 0.40, 1.0];
const FINE_COLOUR: [f32; 4] = [0.42, 0.52, 0.66, 1.0];

/// What the command line asked for.
#[derive(Resource, Clone, Debug)]
struct Args {
    sub: usize,
    wire: bool,
    eye: Option<Vec3>,
    look: Option<Vec3>,
    shot: Option<String>,
    frames: u32,
}

fn parse_args() -> Args {
    let mut args = Args {
        sub: 4,
        wire: false,
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
        .add_plugins(WireframePlugin::default())
        .insert_resource(WireframeConfig {
            global: args.wire,
            default_color: Color::srgb(0.1, 0.1, 0.12),
        })
        .insert_resource(args)
        .add_systems(Startup, spawn_world)
        .add_systems(Update, (grab_mouse, fly, toggle_wireframe, take_shot))
        .run();
}

/// The camera's heading, kept as angles so a look cannot roll.
#[derive(Component)]
struct Fly {
    yaw: f32,
    pitch: f32,
}

/// The planet, and the slab and wall on its top.
fn world(top_of: impl Fn(&Planet) -> f64) -> (Planet, Vec<Block>, f64) {
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
    (planet, blocks, top)
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

/// A corner whose normal is further than this from its triangle's face is
/// shaded on the face's normal, radians: a box's edge, never a slope of
/// ground.
const CREASE: f32 = 0.35;

/// The chunk as Bevy draws it. Vertices are split per triangle, and each
/// corner keeps the smooth normal the field gave it unless that normal
/// disagrees with the triangle's face by more than a crease, in which case
/// it takes the face's: a box face is shaded on its own plane and meets the
/// next on an edge, the ground stays round, and where the ground meets a
/// wall only the corner on the crease changes, so the shading on either
/// side of it is continuous.
fn to_mesh(m: &DcMesh) -> Mesh {
    let mut positions = Vec::with_capacity(m.indices.len());
    let mut normals = Vec::with_capacity(m.indices.len());
    let mut colours = Vec::with_capacity(m.indices.len());
    for t in m.indices.chunks(3) {
        let p: Vec<Vec3> = t
            .iter()
            .map(|&i| Vec3::from(m.positions[i as usize]))
            .collect();
        let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        for (k, &i) in t.iter().enumerate() {
            let n = Vec3::from(m.normals[i as usize]);
            positions.push(p[k].to_array());
            normals.push(if n.angle_between(face) > CREASE {
                face.to_array()
            } else {
                n.to_array()
            });
            colours.push(if m.levels[i as usize] == 0 {
                COARSE_COLOUR
            } else {
                FINE_COLOUR
            });
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    args: Res<Args>,
) {
    let (planet, blocks, top) = world(top_of);
    let built = Built {
        ground: &planet,
        blocks,
    };
    let half = CELLS as f64 * CELL * 0.5;
    let offset = 0.5 * CELL / args.sub as f64;
    let mut lat = Lattice::new(DVec3::splat(-half + offset), CELL, args.sub, CELLS);
    let site = DVec3::new(0.0, top, 0.0);
    let masked = lat.subdivide_near(site, SITE_REACH);
    let grown = lat.grow(&built);
    info!(
        "lattice: {}^3 cells of {} m, {} fine under the site at y = {:.2} and {} grown, {} m fine cells",
        CELLS,
        CELL,
        masked,
        top,
        grown,
        lat.fine
    );
    let (chunks, _) = build(&built, &lat);
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        ..default()
    });
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
    let eye = args.eye.unwrap_or(Vec3::new(8.0, top as f32 + 3.0, 8.0));
    let look = args.look.unwrap_or(Vec3::new(0.0, top as f32 + 0.8, 0.0));
    let d = (look - eye).normalize_or(Vec3::NEG_Z);
    let fly = Fly {
        yaw: (-d.x).atan2(-d.z),
        pitch: d.y.clamp(-1.0, 1.0).asin(),
    };
    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 10.5 },
        Transform::from_translation(eye).with_rotation(Quat::from_euler(
            EulerRot::YXZ,
            fly.yaw,
            fly.pitch,
            0.0,
        )),
        fly,
    ));
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

/// Radians of look per pixel of mouse.
const LOOK: f32 = 0.0022;
/// Metres a second, and the factor Shift puts on it.
const SPEED: f32 = 4.0;
const SPRINT: f32 = 4.0;

/// Fly the camera: the mouse turns it while it is taken, and the keys move
/// it in its own frame.
fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    cursor: Query<&CursorOptions, With<PrimaryWindow>>,
    mut cam: Query<(&mut Transform, &mut Fly)>,
) {
    let Ok((mut tf, mut fly)) = cam.single_mut() else {
        return;
    };
    let taken = cursor
        .single()
        .map(|c| c.grab_mode == CursorGrabMode::Locked)
        .unwrap_or(false);
    for m in motion.read() {
        if !taken {
            continue;
        }
        // One event carrying a whole screen is a window handing focus back,
        // never a look; clamp it rather than turn twice round.
        let d = m.delta.clamp(Vec2::splat(-200.0), Vec2::splat(200.0));
        fly.yaw -= d.x * LOOK;
        fly.pitch = (fly.pitch - d.y * LOOK).clamp(-1.5, 1.5);
    }
    tf.rotation = Quat::from_euler(EulerRot::YXZ, fly.yaw, fly.pitch, 0.0);
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
    let step = v.normalize_or_zero() * speed * time.delta_secs().min(0.1);
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
