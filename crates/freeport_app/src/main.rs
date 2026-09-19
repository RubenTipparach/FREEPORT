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
//! freeport_app [--wire] [--lod-wire] [--fly] [--eye x,y,z] [--look x,y,z] [--levels N]
//!              [--fps N] [--octaves N] [--walk N] [--shot out.png]
//!              [--frames N] [--bake-atlas]
//! ```
//!
//! Left click takes the mouse, Escape gives it back. On foot: WASD, Shift
//! runs, Space jumps. Flying: WASD, Space/Ctrl rise/sink, Q/E roll, Shift
//! boosts, mouse wheel sets cruise speed, R levels the view. N selects a
//! destination and G faces it. Atmospheres limit speed before landing. F swaps the
//! two, Tab toggles the wireframe, L colors terrain LODs and K freezes their
//! rings for inspection. `--eye` is where to start, metres from
//! the system origin (on foot, the spot under it) and `--look` what to
//! face; both default to the port.
mod args;
mod atlas;
mod buildings;
mod city;
mod compute;
mod distant;
mod flight_bench;
mod fly;
mod lamps;
mod lod_debug;
mod meshing;
mod planet_view;
mod planets;
mod render_probe;
mod sky;
mod stream;
mod terrain;
mod traffic;
mod tuning;
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
use fly::{fly, FlightSettings, Fly};
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
/// The sea's radius. MEASURED rather than picked: the owner's ask is a
/// body at least half water, and a sea level is a percentile of the
/// body's own height distribution, not a number that means anything on
/// its own. Over 40,000 directions of this planet the relief spans
/// -2,920 to 4,358 m and its median is +351, so a sea at -400 m left the
/// world 26.3% water, which is a continent with lakes in it, and at
/// +820 m it was 65%, which is an ocean with a few scraps in it.
///
/// It is +1,000 m now, which is 62.1% water: a little less than it was,
/// and where this body's land is SEVEN continents rather than one blob
/// and three hundred scraps. Which of those two things a sea level buys
/// is not a property of the level at all, it is a property of the
/// continental SHELF under it (`biome::SHELF_AT` and its neighbours), and
/// the two were picked together off one sweep:
/// `the_land_is_a_few_continents_and_many_islands` counts the body's
/// connected landmasses, `measure_the_land_at_each_sea_level` is the
/// sweep, and `the_harness_planet_is_mostly_water` holds the half the ask
/// names. Lower, the continents MERGE: at +700 m the body is 54.6% water
/// and nearly all of its land is one mass, which is percolation rather
/// than a tuning mistake.
const SEA: f64 = RADIUS + 1100.0;
/// Towns: how many, and how far across each.
/// How many towns are PLANNED on the planet. Every one of them levels its
/// own ground and is painted on the body's chart, so a world with this
/// many has cities all over it from orbit and level ground waiting under
/// each of them.
const TOWNS: usize = 160;
/// How far across the BIGGEST town on the body is, metres. Every other
/// town's size falls off its own rank by Zipf's law (`town::size_of`),
/// so this is a ceiling rather than the one figure every city was: at 80
/// m, which is what it was, a hundred and sixty settlements were a
/// hundred and sixty copies of one settlement.
const TOWN_RADIUS: f64 = 170.0;

/// How many of the planned towns are BUILT, nearest to where the world
/// starts first.
///
/// A town is about 375,000 triangles of baked buildings, so the eight
/// this world had were three million of them and a hundred and sixty
/// would be sixty million, which is not a thing to hold. Planning is
/// cheap and building is not, so every town is planned, levels its own
/// ground and is painted on the body's chart, and the nearest few are
/// built. A COUNT rather than a distance, because it is the count that
/// bounds the cost: at a hundred and sixty towns on this planet the mean
/// spacing is 280 km, so a reach of ninety thousand metres built exactly
/// one of them.
///
/// What is MISSING and named rather than hidden: a town that comes into
/// range as you fly is not built, so a far city is its own levelled
/// plateau with no buildings on it until town streaming lands. The chunk
/// streamer already does exactly this for ground and the shape of it is
/// the same.
const TOWNS_BUILT: usize = 8;
/// The world's seed.
const SEED: u32 = 7;
/// Ten levels at 0.5 m preserve the previous 32.8 km streaming box while
/// removing the unnecessarily dense 0.25 m tier. Distant meshes fill the disk.
const LEVELS: u8 = 10;
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
/// read by the light that casts the shadows, by the sky dome, by the fog
/// and by the bodies drawn from far off, so they cannot point four ways.
///
/// It is asked about the WALKER's own start and never about where `aim`
/// put the camera, which is a circle: `--sunward` stands the camera along
/// the sun, so a sun measured over that camera is a sun measured over
/// itself. Measured on this planet, `--sunward 2.6 --around 150` put the
/// camera 58 degrees from the sun rather than 150, and the picture of the
/// body's own midnight came back three quarters lit.
/// Where the camera starts: on a street of the port, or, with
/// `--sunward`, that many radii off the body and looking at its centre,
/// `--around` degrees round from the sun.
///
/// A camera for a picture is SOLVED and never hand aimed, which is
/// tenebris's LODCAM lesson: the sun stands over wherever the world
/// starts, so where it is depends on where the towns came out, and three
/// runs of this were aimed by hand at a planet that turned out to be a
/// different one, in its own night.
///
/// `--around` is the same rule for the NIGHT side. A body's dark half is
/// a picture nobody can aim at either, because where it is depends on
/// where the sun came out: turned 180 degrees the camera is at the body's
/// own midnight and 140 leaves a crescent of day in the frame, which is
/// what shows the lights and the ground they stand on in one picture.
fn aim(world: &World, args: &Args) -> (DVec3, DVec3) {
    let (eye, look) = start(world);
    // Straight down on the PORT, which is the one camera a town's own
    // plan can be judged from: its outline, its zones and where its
    // streets run are a thing seen from above and nothing else.
    if let Some(over) = args.over {
        if let Some(port) = world.towns.first() {
            let ground = port.dir * (world.planet.radius + port.h);
            return (ground + port.dir * over, ground);
        }
    }
    match args.sunward {
        Some(radii) => (
            turned(sun_over(eye), args.around.to_radians()) * world.planet.radius * radii.max(1.05),
            DVec3::ZERO,
        ),
        None => (eye, look),
    }
}

/// A direction turned `angle` away from itself, about whichever axis is
/// square to it. Which axis does not matter for a picture of a sphere:
/// what is being asked for is how much of the body's night is in frame,
/// and that is the ANGLE alone.
fn turned(dir: DVec3, angle: f64) -> DVec3 {
    let (east, _) = town::frame_at(dir);
    (dir * angle.cos() + east * angle.sin()).normalize_or(DVec3::Y)
}

fn sun_over(dir: DVec3) -> DVec3 {
    let (east, north) = town::frame_at(dir);
    let up = SUN_UP.to_radians();
    let round = SUN_BEARING.to_radians();
    (dir.normalize_or(DVec3::Y) * up.sin() + (north * round.cos() + east * round.sin()) * up.cos())
        .normalize_or(DVec3::Y)
}

/// Radians of look per pixel of mouse.
pub(crate) const LOOK: f32 = 0.0022;

fn main() {
    let args = parse_args();
    if args.bake_atlas {
        world::bake_atlas(&args);
        return;
    }
    let lod_debug = lod_debug::LodDebug {
        enabled: args.lod_wire,
        frozen: false,
    };
    let mut app = App::new();
    let present_mode = if args.benchmark.is_some() {
        bevy::window::PresentMode::AutoNoVsync
    } else {
        default()
    };
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            present_mode,
            ..default()
        }),
        ..default()
    }));
    if args.benchmark.is_some() {
        app.insert_resource(bevy::winit::WinitSettings::continuous());
    }
    if args.profile_render {
        app.add_plugins(render_probe::RenderProbePlugin);
    }
    app.add_plugins((
        WireframePlugin::default(),
        TerrainPlugin,
        distant::DistantPlugin,
        WaterPlugin,
        sky::SkyPlugin,
    ))
    .insert_resource(WireframeConfig {
        global: args.wire && !args.lod_wire,
        default_color: Color::srgb(0.1, 0.1, 0.12),
    })
    // What is BEHIND the sky is space, and the sky is the air: a
    // painted blue would be a second answer to what the sky looks
    // like, and the one that could not be right at dusk.
    .insert_resource(ClearColor(Color::srgb(0.01, 0.012, 0.02)))
    .insert_resource(args)
    .insert_resource(lod_debug)
    .insert_resource(tuning::Tuning::load())
    .insert_resource(FlightSettings::load())
    .init_resource::<Eye>()
    .init_resource::<Frame>()
    .init_resource::<Status>()
    .init_resource::<flight_bench::Benchmark>()
    .add_systems(
        Startup,
        (compute::init_compute, spawn_world, flight_bench::setup).chain(),
    )
    .add_systems(
        PreUpdate,
        flight_bench::clear_input.after(bevy::input::InputSystems),
    )
    .add_systems(Startup, lod_debug::spawn_legend)
    .add_systems(Last, flight_bench::after_update);
    tick(&mut app);
    app.run();
}

/// Everything a frame DOES, in the order it does it. Its own function
/// rather than another link on the builder, because `main` is the
/// arguments, the `App` and the schedule, and the schedule is the long
/// one of the three.
fn tick(app: &mut App) {
    app // Bevy takes at most twenty systems in one tuple and this schedule is
        // past it, so it is two tuples each chained, chained to each other.
        // TWO `add_systems` calls would NOT have done: Bevy gives no order
        // between two registrations, and `place_eye` before `stream` is a
        // frame drawn from where the eye was going to be.
        .add_systems(
            Update,
            (
                (
                    grab_mouse,
                    lod_debug::controls,
                    toggle_walk,
                    walk,
                    fly,
                    flight_bench::drive,
                    planets::activate,
                    rebase_origin,
                    planet_view::recentre,
                    city::update_lod,
                    stream,
                    flight_bench::after_stream,
                    light_lamps,
                    traffic::drive_traffic,
                )
                    .chain(),
                (
                    place_eye,
                    sky::drift_sky,
                    show_status,
                    lod_debug::apply,
                    take_shot,
                    flight_bench::finish,
                    hold_frame,
                )
                    .chain(),
            )
                .chain(),
        );
}

/// What a frame of input is read from: the clock, the keys, the mouse's
/// motion and whether the window holds it. One thing, so a system that
/// reads the player is not a system with eight arguments.
#[derive(SystemParam)]
pub(crate) struct Controls<'w, 's> {
    pub time: Res<'w, Time>,
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    motion: MessageReader<'w, 's, MouseMotion>,
    cursor: Query<'w, 's, (&'static CursorOptions, &'static Window), With<PrimaryWindow>>,
}

impl Controls<'_, '_> {
    pub fn focused(&self) -> bool {
        self.cursor.single().is_ok_and(|(_, window)| window.focused)
    }

    /// This frame's look, radians about the local up and of tilt, from the
    /// mouse while the window holds it. One event carrying a whole screen
    /// is a window handing focus back, never a look; it is clamped rather
    /// than turned twice round.
    pub fn look(&mut self) -> Vec2 {
        let taken = self
            .cursor
            .single()
            .map(|(c, window)| window.focused && c.grab_mode == CursorGrabMode::Locked)
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

#[derive(SystemParam)]
struct WorldAssets<'w> {
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<TerrainMaterial>>,
    waters: ResMut<'w, Assets<WaterMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    skies: ResMut<'w, Assets<sky::Sky>>,
    standard: ResMut<'w, Assets<StandardMaterial>>,
    distants: ResMut<'w, Assets<distant::DistantMaterial>>,
}

fn spawn_world(
    mut commands: Commands,
    assets: WorldAssets,
    args: Res<Args>,
    mut compute: ResMut<compute::Compute>,
    tuning: Res<tuning::Tuning>,
    flight: Res<FlightSettings>,
) {
    let WorldAssets {
        mut images,
        mut materials,
        mut waters,
        mut meshes,
        mut distants,
        mut skies,
        mut standard,
    } = assets;
    let (world, towns) = world::build(&args);
    let (start_eye, start_look) = aim(&world, &args);
    // The sun, worked out while the world is still here to ask: it is a
    // fact about where the WALKER starts, and `start_eye` is wherever the
    // camera was aimed.
    let sun = sun_over(world::start(&world).0);
    let eye = args.eye.unwrap_or(start_eye);
    let look = args.look.unwrap_or(start_look);
    // The lattice's origin sits half a fine cell off the half metre grid
    // that everything built snaps to, so no face of a block ever lies on
    // a lattice plane (a face that does puts its crease on a lattice edge
    // and the cells either side of it solve to one point, which the core's
    // audit counts as a pinch), and far enough out that every index over
    // the planet is positive.
    let fine = args.cell_size.unwrap_or(tuning.terrain_cell_size);
    let corner = DVec3::splat(-2.0 * RADIUS - 100.0 + 0.5 * fine);
    let lat = Lattice::new(corner, fine);
    let frames: Vec<_> = world
        .towns
        .iter()
        .map(|t| town::lot_frame(RADIUS, t, 0.0, 0.0))
        .collect();
    let material = terrain_material(&mut images, &mut materials, &frames, SEA as f32);
    let sheet = water_material(&mut waters, SEA);
    say_world(&world, start_eye, &lat, args.levels);
    // The people and the cars on their streets. Built from the same
    // towns, so a crowd exists exactly where the buildings do.
    traffic::turn_out(
        &mut commands,
        0,
        &world.built,
        SEED,
        &mut meshes,
        &mut standard,
    );
    // The towns are drawn once and never again: models, not chunks.
    city::spawn_towns(
        &mut commands,
        &mut meshes,
        &material,
        &Frame::default(),
        SEA,
        towns,
        &mut standard,
    );
    let mut planets = planets::Planets::load(Arc::new(world));
    planets.bodies[0].material = material;
    planets.bodies[0].water = sheet;
    planet_view::spawn(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut waters,
        &mut images,
        &mut distants,
        &mut planets,
    );
    planets.active = planets.nearest(eye);
    let body = &planets.bodies[planets.active];
    spawn_streamer(&mut commands, lat, eye, &args, body, &mut compute, &tuning);
    // The sun, and everything that reads it: the WORLD's own start decides
    // which way it points and never `--eye`, so two pictures taken from
    // two places are lit the same and only the camera moved. The light,
    // the dome and the fog are handed one direction worked out once.
    spawn_light(&mut commands, sun);
    spawn_status(&mut commands);
    let env = spawn_sky(
        &mut commands,
        &mut meshes,
        &mut skies,
        &mut images,
        eye - body.centre,
        sky::Weather {
            air: body.air,
            sea: body.world.sea.radius,
            sun,
        },
    );
    spawn_camera(&mut commands, body, &args, eye, look, env, &flight);
    commands.insert_resource(Eye(WorldPos(eye)));
    commands.insert_resource(Ground(body.world.clone(), body.centre));
    commands.insert_resource(planets);
}

/// The chunk streamer, standing at whichever body the eye is nearest.
/// Its own function because it is the one thing in `spawn_world` that
/// reaches into three resources to make one.
fn spawn_streamer(
    commands: &mut Commands,
    lat: Lattice,
    eye: DVec3,
    args: &Args,
    body: &planets::Body,
    compute: &mut compute::Compute,
    tuning: &tuning::Tuning,
) {
    let mut streamer = Streamer::new(
        lat,
        eye - body.centre,
        args.levels,
        body.material.clone(),
        body.water.clone(),
        compute.0.take(),
        tuning.clone(),
    );
    streamer.centre = body.centre;
    commands.insert_resource(streamer);
}

fn say_world(world: &World, eye: DVec3, lat: &Lattice, levels: u8) {
    let says = match shore(world, eye) {
        Some(s) => format!(", the shore {:.0} m off at {s:.0}", (s - eye).length()),
        None => ", no shore within a quarter turn".to_string(),
    };
    info!(
        "planet of {} m, the sea at {} m, {} levels of {} m to {} m cells, the eye at {:.0}{}",
        RADIUS,
        SEA,
        levels,
        lat.cell(0),
        lat.cell(levels - 1),
        eye,
        says
    );
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
    weather: sky::Weather,
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
        bevy::camera::visibility::RenderLayers::from_layers(&[0, 1]),
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
            right: Val::Px(12.0),
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
    body: &planets::Body,
    args: &Args,
    eye: DVec3,
    look: DVec3,
    env: Handle<Image>,
    flight: &FlightSettings,
) {
    let d = (look - eye).normalize_or(DVec3::NEG_Z);
    let local = eye - body.centre;
    let world = &body.world;
    let fly = Fly::new(eye, d, local.normalize_or(DVec3::Y), flight.speed);
    if !args.fly {
        let w = Walker::enter(&world.underfoot(local, 8.0), &world.bounds, local, d);
        commands.insert_resource(OnFoot(w));
    }
    commands.spawn((
        Camera3d {
            screen_space_specular_transmission_steps: 1,
            ..default()
        },
        DepthPrepass,
        Projection::Perspective(PerspectiveProjection {
            far: 100_000_000.0,
            ..default()
        }),
        Exposure { ev100: 10.5 },
        // The sky lights the world: what fills a shadow is the air over
        // it, off the same march the dome is drawn by, which is why a
        // face turned away from the sun is sky blue and not black.
        bevy::light::GeneratedEnvironmentMapLight {
            environment_map: env,
            intensity: 1.0,
            ..default()
        },
        sky::StaticEnvironment,
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
        None => Transform::from_translation(at).with_rotation(fly.rotation),
    };
}

fn show_status(
    status: Res<Status>,
    streamer: Option<Res<Streamer>>,
    walker: Option<Res<OnFoot>>,
    mut text: Query<&mut Text, With<Stat>>,
) {
    let what = streamer.map(|s| s.status()).unwrap_or_default();
    let mode = if walker.is_some() { "fly" } else { "walk" };
    if let Ok(mut text) = text.single_mut() {
        text.0 = format!(
            "{}\n{}   |   F {mode}, Tab wire, L LOD, Esc mouse",
            status.walker, what
        );
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
    if args.fps <= 0.0 || args.shot.is_some() || args.benchmark.is_some() {
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
    mut shot: Local<ShotState>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &args.shot else {
        return;
    };
    shot.frame += 1;
    let now = Instant::now();
    if let Some(last) = shot.last.replace(now) {
        shot.times.push((now - last).as_secs_f64() * 1000.0);
    }
    // The picture waits for the ground: every chunk the rings want drawn
    // once, or ten times the frames asked for, whichever comes first.
    let ready = match &streamer {
        Some(s) => s.idle() || shot.frame >= args.frames * 10,
        None => true,
    };
    if shot.taken.is_none() && shot.frame >= args.frames && ready {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        shot.taken = Some(shot.frame);
        shot.save_metrics(path, streamer.as_deref());
    }
    if shot.taken.is_some_and(|t| shot.frame >= t + 12) {
        exit.write(AppExit::Success);
    }
}

#[derive(Default)]
struct ShotState {
    frame: u32,
    taken: Option<u32>,
    last: Option<Instant>,
    times: Vec<f64>,
}

impl ShotState {
    fn save_metrics(&self, path: &str, streamer: Option<&Streamer>) {
        let mut times = self.times.clone();
        times.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            times
                .get(((times.len().saturating_sub(1)) as f64 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let value = serde_json::json!({
            "frames": self.frame, "frame_p50_ms": percentile(0.5), "frame_p95_ms": percentile(0.95),
            "frame_max_ms": times.last(), "terrain": streamer.map(Streamer::measurement),
        });
        let destination = std::path::Path::new(path).with_extension("metrics.json");
        if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
            if let Err(e) = std::fs::write(destination, bytes) {
                warn!("could not write screenshot metrics: {e}");
            }
        }
    }
}
