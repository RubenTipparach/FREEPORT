//! freeport_app: the Bevy harness. It draws what `freeport_core` says.
//!
//! A planet two thousand kilometres across, drawn ONE way: a density field
//! dual contoured a chunk at a time (`freeport_core::dc`) on one lattice at
//! every level, in rings round the eye streamed on worker threads
//! (`stream.rs`). The walker stands on that same field, so the picture is
//! the collider. The ground wears the baked sets (`terrain.rs`), the sea is
//! that mesher's own surface under tenebris's water shader (`water.rs`),
//! the towns on it are parametric models (`freeport_core::model`), and all
//! of it stands under one sky (`sky.rs`) whose march is the core's. There
//! is a fly camera, a wireframe toggle, a frame cap and a screenshot flag
//! so a picture can be taken headless under Xvfb and lavapipe. The eye is a
//! world position in `f64` and the camera is placed from it through the
//! floating origin.
//!
//! ```text
//! freeport_app [--wire] [--fly] [--eye x,y,z] [--look x,y,z] [--levels N]
//!              [--fps N] [--octaves N] [--walk N] [--shot out.png]
//!              [--frames N]
//! ```
//!
//! Left click takes the mouse, Escape gives it back. On foot: WASD, Shift
//! runs, Space jumps. Flying: WASD and Q E, Shift is faster. F swaps the
//! two, Tab toggles the wireframe. `--eye` is where to start, metres from
//! the planet's centre (on foot, the spot under it) and `--look` what to
//! face; both default to the port.
mod args;
mod city;
mod lamps;
mod sky;
mod stream;
mod terrain;
mod walk;
mod water;
mod world;

use args::{parse_args, Args};
use bevy::camera::Exposure;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::MouseMotion;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::math::DVec3;
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::lattice::Lattice;
use freeport_core::pos::WorldPos;
use freeport_core::town;
use freeport_core::walker::Walker;
use lamps::light_lamps;
use std::sync::Arc;
use std::time::Instant;
use stream::{rebase_origin, stream, Frame, Streamer};
use terrain::{terrain_material, TerrainMaterial, TerrainPlugin};
use walk::{toggle_walk, walk, OnFoot};
use water::{water_material, WaterMaterial, WaterPlugin};
use world::{shore, start};
pub(crate) use world::{Ground, World};

/// The planet: a thousand kilometres of radius, so two thousand across,
/// which is a small terrestrial world and the scale `CLAUDE.md`'s table
/// asks for. What it costs the streamer is nothing, because the rings are
/// a fixed count of chunks round the eye however big the ball under them
/// is; what it costs is OCTAVES, because the same detail on the ground is
/// further down a fractal, and PRECISION, which is why a chunk's mesh is
/// metres from its own `f64` corner.
const RADIUS: f64 = 1_000_000.0;
/// Peak to trough of the relief, metres: eight tenths of a percent of the
/// radius, which is about what a terrestrial world carries.
const RELIEF: f64 = 8_000.0;
/// How many relief features fit round the planet, and how many octaves
/// take that down to metres: `log2(2 pi R / lumps / 2 m)` is eighteen
/// here against eleven on a five kilometre world, which is the one real
/// cost of the bigger planet.
const LUMPS: f64 = 12.0;
const OCTAVES: u32 = 18;
/// The sea's level, metres under the mean radius.
const SEA: f64 = RADIUS - 400.0;
/// Towns: how many, and how far across each.
const TOWNS: usize = 8;
const TOWN_RADIUS: f64 = 80.0;
/// The world's seed.
const SEED: u32 = 7;
/// The finest cell, metres, under the feet.
const FINE: f64 = 0.25;
/// Levels of rings: the coarsest box is `16 * 8 * FINE * 2^(LEVELS-1)`
/// across, 32 km at eleven, which holds the whole planet from any eye on
/// it.
const LEVELS: u8 = 11;
/// Frames a second the loop is held to by default. Vsync is the MONITOR's
/// cap and not a cap at all: a scene this cheap to simulate draws at the
/// refresh rate and holds the card at full clock the whole time, which is
/// a hot room for frames nobody asked for. swarm-demo's number and its
/// rule, `--fps 0` lifts it.
const FPS: f64 = 144.0;

/// Where the sun stands over the WORLD's own starting point: degrees over
/// the local horizon there, and degrees round from local north. A low sun
/// is the light a landscape reads best in, and the bearing puts it off the
/// shoulder rather than behind the camera. It is the world's start and
/// never `--eye`, so two pictures from two places are lit alike.
///
/// It was a fixed world direction, and its own comment claimed it stood
/// "a little over the horizon at the harness's start", which is a thing a
/// world direction cannot promise: it is true of one spot on the planet
/// and the towns are placed by the ground. On this planet the port came
/// out 56 degrees into its own NIGHT, and a picture of a city at midnight
/// is a picture of nothing. `sun_over` measures it from where the world
/// starts instead, so the claim is kept by construction on any planet,
/// any seed and any port.
const SUN_UP: f64 = 32.0;
const SUN_BEARING: f64 = 40.0;

/// The sun's world direction for an eye starting at `dir`: ONE number,
/// read by the light that casts the shadows, by the sky dome and by the
/// fog, so the three cannot point three ways.
fn sun_over(dir: DVec3) -> DVec3 {
    let (east, north) = town::frame_at(dir);
    let up = SUN_UP.to_radians();
    let round = SUN_BEARING.to_radians();
    (dir.normalize_or(DVec3::Y) * up.sin() + (north * round.cos() + east * round.sin()) * up.cos())
        .normalize_or(DVec3::Y)
}

/// Radians of look per pixel of mouse.
pub(crate) const LOOK: f32 = 0.0022;
/// Flying: metres a second, and the factor Shift puts on it.
const SPEED: f64 = 6.0;
const SPRINT: f64 = 8.0;

fn main() {
    let args = parse_args();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            WireframePlugin::default(),
            TerrainPlugin,
            WaterPlugin,
            sky::SkyPlugin,
        ))
        .insert_resource(WireframeConfig {
            global: args.wire,
            default_color: Color::srgb(0.1, 0.1, 0.12),
        })
        // What is BEHIND the sky is space, and the sky is the air: a
        // painted blue would be a second answer to what the sky looks
        // like, and the one that could not be right at dusk.
        .insert_resource(ClearColor(Color::srgb(0.01, 0.012, 0.02)))
        .insert_resource(args)
        .init_resource::<Eye>()
        .init_resource::<Frame>()
        .init_resource::<Status>()
        .add_systems(Startup, spawn_world)
        .add_systems(
            Update,
            (
                grab_mouse,
                toggle_walk,
                walk,
                fly,
                rebase_origin,
                stream,
                light_lamps,
                place_eye,
                sky::drift_sky,
                show_status,
                toggle_wireframe,
                take_shot,
                hold_frame,
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

/// The fly camera: where it is in the world frame and its heading, kept as
/// angles so a look cannot roll.
#[derive(Component)]
pub(crate) struct Fly {
    pub yaw: f32,
    pub pitch: f32,
    pub at: DVec3,
}

impl Fly {
    /// The way it faces.
    pub fn forward(&self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0) * Vec3::NEG_Z
    }
}

/// Where the eye is, in the world frame: the walker's or the fly camera's.
#[derive(Resource, Default)]
pub(crate) struct Eye(pub WorldPos);

/// The status line's parts.
#[derive(Resource, Default)]
pub(crate) struct Status {
    pub walker: String,
}

/// The line of text that says where the walker stands.
#[derive(Component)]
struct Stat;

fn spawn_world(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut skies: ResMut<Assets<sky::Sky>>,
    args: Res<Args>,
) {
    let (world, towns) = world::build(&args);
    let (start_eye, start_look) = start(&world);
    let eye = args.eye.unwrap_or(start_eye);
    let look = args.look.unwrap_or(start_look);
    // The lattice's origin sits half a fine cell off the half metre grid
    // that everything built snaps to, so no face of a block ever lies on
    // a lattice plane (a face that does puts its crease on a lattice edge
    // and the cells either side of it solve to one point, which the core's
    // audit counts as a pinch), and far enough out that every index over
    // the planet is positive.
    let corner = DVec3::splat(-2.0 * RADIUS - 100.0 + 0.5 * FINE);
    let lat = Lattice::new(corner, FINE);
    let frames: Vec<_> = world
        .towns
        .iter()
        .map(|t| town::lot_frame(RADIUS, t, 0.0, 0.0))
        .collect();
    let material = terrain_material(&mut images, &mut materials, &frames, SEA as f32);
    let sheet = water_material(&mut waters, SEA);
    // Where the sea is, so a picture can be aimed at it. A hunt that finds
    // nothing says so rather than printing a NaN somebody has to work out
    // the meaning of.
    let says = match shore(&world, eye) {
        Some(s) => format!(", the shore {:.0} m off at {s:.0}", (s - eye).length()),
        None => ", no shore within a quarter turn".to_string(),
    };
    info!(
        "planet of {} m, the sea at {} m, {} levels of {} m to {} m cells, the eye at {:.0}{}",
        RADIUS,
        SEA,
        args.levels,
        lat.cell(0),
        lat.cell(args.levels - 1),
        eye,
        says,
    );
    commands.insert_resource(Streamer::new(
        lat,
        eye,
        args.levels,
        SEA,
        material.clone(),
        sheet,
    ));
    // The towns are drawn once and never again: models, not chunks.
    city::spawn_towns(
        &mut commands,
        &mut meshes,
        &material,
        &Frame::default(),
        SEA,
        towns,
    );
    // The sun, and everything that reads it: the WORLD's own start decides
    // which way it points and never `--eye`, so two pictures taken from
    // two places are lit the same and only the camera moved. The light,
    // the dome and the fog are handed one direction worked out once.
    let sun = sun_over(start_eye);
    spawn_light(&mut commands, sun);
    spawn_status(&mut commands);
    let env = spawn_sky(
        &mut commands,
        &mut meshes,
        &mut skies,
        &mut images,
        eye,
        sun,
    );
    spawn_camera(&mut commands, &world, &args, eye, look, env);
    commands.insert_resource(Eye(WorldPos(eye)));
    commands.insert_resource(Ground(Arc::new(world)));
}

/// The sky this world stands under: the air and the sun as one resource,
/// the dome that draws them, the floor of ambient under it, and the same
/// march baked into a cubemap for the camera to wear. Answers that
/// cubemap, which is the one thing a caller needs back.
fn spawn_sky(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    skies: &mut Assets<sky::Sky>,
    images: &mut Assets<Image>,
    eye: DVec3,
    sun: DVec3,
) -> Handle<Image> {
    commands.insert_resource(GlobalAmbientLight {
        // Nearly nothing: what fills a shadow is the SKY, through the
        // baked cubemap on the camera, and a flat ambient over the top of
        // it would be a second answer to the same question. What is left
        // is a floor, so a face with no sky over it is the colour of the
        // gap between two stars rather than a hole in the picture, which
        // is swarm-demo's own lesson about a nought ambient.
        brightness: 8.0,
        ..default()
    });
    let weather = sky::Weather {
        air: freeport_core::atmos::Air::round(RADIUS, RELIEF),
        sea: SEA,
        sun,
    };
    sky::spawn_dome(commands, meshes, skies, &weather);
    let lit = Instant::now();
    let env = images.add(sky::bake_env(&weather.air, weather.sun, eye));
    info!(
        "the sky baked into a cubemap in {} ms",
        lit.elapsed().as_millis()
    );
    commands.insert_resource(weather);
    env
}

/// The sun, and the light it casts. The direction is the weather's own and
/// nothing else, so the shadows fall the way the sky says they should.
fn spawn_light(commands: &mut Commands, sun: DVec3) {
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 12.0,
            maximum_distance: 500.0,
            ..default()
        }
        .build(),
        // The light shines the way the sun is NOT: Bevy's forward is
        // negative Z and a directional light travels along it.
        Transform::from_translation(Vec3::ZERO).looking_to(-sun.as_vec3(), Vec3::Y),
    ));
}

/// The line of text along the bottom that says where the eye is.
fn spawn_status(commands: &mut Commands) {
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
}

/// The camera, flying from `eye` toward `look`, or on foot at the spot
/// under `eye` facing `look`.
fn spawn_camera(
    commands: &mut Commands,
    world: &World,
    args: &Args,
    eye: DVec3,
    look: DVec3,
    env: Handle<Image>,
) {
    let d = (look - eye).normalize_or(DVec3::NEG_Z);
    let fly = Fly {
        yaw: (-d.x as f32).atan2(-d.z as f32),
        pitch: (d.y as f32).clamp(-1.0, 1.0).asin(),
        at: eye,
    };
    if !args.fly {
        let w = Walker::enter(&world.underfoot(eye, 8.0), &world.bounds, eye, d);
        commands.insert_resource(OnFoot(w));
    }
    commands.spawn((
        Camera3d {
            screen_space_specular_transmission_steps: 1,
            ..default()
        },
        DepthPrepass,
        Exposure { ev100: 10.5 },
        // The sky lights the world: what fills a shadow is the air over
        // it, off the same march the dome is drawn by, which is why a
        // face turned away from the sun is sky blue and not black.
        bevy::light::GeneratedEnvironmentMapLight {
            environment_map: env,
            intensity: 1.0,
            ..default()
        },
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

/// Fly the camera while nobody is on foot: the mouse turns it while it is
/// taken, and the keys move it in its own frame, in the world frame's
/// `f64`.
fn fly(
    mut controls: Controls,
    on_foot: Option<Res<OnFoot>>,
    mut cam: Query<&mut Fly>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
) {
    let look = controls.look();
    if on_foot.is_some() {
        return;
    }
    let Ok(mut fly) = cam.single_mut() else {
        return;
    };
    fly.yaw -= look.x;
    fly.pitch = (fly.pitch - look.y).clamp(-1.5, 1.5);
    let rot = Quat::from_euler(EulerRot::YXZ, fly.yaw, fly.pitch, 0.0);
    let keys = &controls.keys;
    let mut v = Vec3::ZERO;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
    v += (rot * Vec3::NEG_Z) * axis(KeyCode::KeyS, KeyCode::KeyW);
    v += (rot * Vec3::X) * axis(KeyCode::KeyA, KeyCode::KeyD);
    v += Vec3::Y * axis(KeyCode::KeyQ, KeyCode::KeyE);
    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        SPEED * SPRINT
    } else {
        SPEED
    };
    let step = v.normalize_or_zero().as_dvec3() * speed * controls.time.delta_secs_f64().min(0.1);
    if step.is_finite() {
        fly.at += step;
    }
    eye.0 = WorldPos(fly.at);
    status.walker = format!(
        "flying at {:.1} m over the mean radius",
        fly.at.length() - RADIUS
    );
}

/// The camera at the eye through the origin: looking where the walker
/// looks with the local up as up, or along the fly camera's heading.
fn place_eye(
    frame: Res<Frame>,
    eye: Res<Eye>,
    walker: Option<Res<OnFoot>>,
    mut cam: Query<(&mut Transform, &Fly), With<Camera3d>>,
) {
    let Ok((mut tf, fly)) = cam.single_mut() else {
        return;
    };
    let at = frame.0.local(eye.0);
    *tf = match walker {
        Some(w) => {
            Transform::from_translation(at).looking_to(w.0.look().as_vec3(), w.0.dir.as_vec3())
        }
        None => Transform::from_translation(at).with_rotation(Quat::from_euler(
            EulerRot::YXZ,
            fly.yaw,
            fly.pitch,
            0.0,
        )),
    };
}

fn show_status(
    status: Res<Status>,
    streamer: Option<Res<Streamer>>,
    mut text: Query<&mut Text, With<Stat>>,
) {
    let what = streamer.map(|s| s.status()).unwrap_or_default();
    if let Ok(mut text) = text.single_mut() {
        text.0 = format!(
            "{}   |   {}   |   F fly, Tab wire, Esc mouse",
            status.walker, what
        );
    }
}

fn toggle_wireframe(keys: Res<ButtonInput<KeyCode>>, mut config: ResMut<WireframeConfig>) {
    if keys.just_pressed(KeyCode::Tab) {
        config.global = !config.global;
    }
}

/// Hold the loop to `--fps`. It is a DEADLINE rather than a fixed sleep,
/// so the cap does not drift, and a frame that has already overrun
/// resyncs the deadline to now rather than chasing it: catching up means
/// running the next few flat out, which is the thing a cap exists to
/// prevent. `sleep` and not a spin, because a spin paces better and burns
/// a core doing it, and burning a core is the problem. swarm-demo's, and
/// it is off under `--shot`, where a headless run wants every frame it
/// can get.
fn hold_frame(args: Res<Args>, mut due: Local<Option<Instant>>) {
    if args.fps <= 0.0 || args.shot.is_some() {
        return;
    }
    let frame = std::time::Duration::from_secs_f64(1.0 / args.fps);
    let now = Instant::now();
    let next = match *due {
        Some(at) if at > now => {
            std::thread::sleep(at - now);
            at + frame
        }
        _ => now + frame,
    };
    *due = Some(next);
}

/// With `--shot`, save the frame the arguments asked for once the streamer
/// has settled (or ten times as many frames on), and leave a few frames
/// later, once the write has had its chance.
fn take_shot(
    mut commands: Commands,
    args: Res<Args>,
    streamer: Option<Res<Streamer>>,
    mut frame: Local<u32>,
    mut taken: Local<Option<u32>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &args.shot else {
        return;
    };
    *frame += 1;
    // The picture waits for the ground: every chunk the rings want drawn
    // once, or ten times the frames asked for, whichever comes first.
    let ready = match &streamer {
        Some(s) => s.idle() || *frame >= args.frames * 10,
        None => true,
    };
    if taken.is_none() && *frame >= args.frames && ready {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        *taken = Some(*frame);
    }
    if taken.is_some_and(|t| *frame >= t + 12) {
        exit.write(AppExit::Success);
    }
}
