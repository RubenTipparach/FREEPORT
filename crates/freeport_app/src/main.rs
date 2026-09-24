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
mod aim;
mod args;
mod atlas;
mod buildings;
mod city;
mod clock;
mod compute;
mod cull;
mod distant;
mod drive;
mod flight_bench;
mod fly;
mod fuel;
mod hud;
mod lamps;
mod lod_debug;
mod map;
mod meshing;
mod planet_view;
mod planets;
mod ram;
mod render_probe;
mod roads;
mod route;
mod shot;
mod sky;
mod status;
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
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use drive::{aim_drive, board, drive_car, show_cars, Thefts};
use fly::{fly, FlightSettings, Fly};
use freeport_core::lattice::Lattice;
use freeport_core::pos::WorldPos;
use freeport_core::town;
use freeport_core::walker::Walker;
use lamps::{dim_lamps, light_lamps};
pub(crate) use status::Status;
use std::sync::Arc;
use std::time::Instant;
use stream::{rebase_origin, stream, Frame, Streamer};
use terrain::{terrain_material, TerrainMaterial, TerrainPlugin};
use walk::{toggle_walk, walk, OnFoot};
use water::{water_material, WaterMaterial, WaterPlugin};
use world::shore;
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
/// town's size falls off how near the SEA it stands (`town::coastal`),
/// so this is a ceiling rather than the one figure every city was: at
/// 80 m, which is what it was, a hundred and sixty settlements were a
/// hundred and sixty copies of one settlement.
///
/// TEN TIMES THE CITY, which is the owner's own ask, and it is the
/// square root of ten on the RADIUS because a city is measured by the
/// ground it covers: 170 m becomes 537 and a town of 176 lots becomes
/// one of 1,580. What that costs is the built set, because a town is
/// its own triangles and `TOWNS_BUILT` of them are standing at once.
///
/// And THREE TIMES that again, which is the owner's second ask ("why is
/// the city so small?"), and it took two things this constant could not
/// buy on its own. A city is drawn a BLOCK at a time now, the far ones
/// as solid blocks (`city::tiles`), so nine times the buildings is not
/// nine times the triangles; and its ground is GRADED to the country
/// rather than levelled to one height (`town::Grade`), because one flat
/// pad six and a half kilometres across is not a thing this planet has:
/// of 240 land candidates the median fall across that outline is 282 m,
/// and not one falls under the fifteen a site may cut.
const TOWN_RADIUS: f64 = 1_611.0;

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
/// The set FOLLOWS THE EYE (`city::stream`), so a town that comes over
/// the horizon as you drive is built and the one behind you is dropped.
/// It was picked ONCE, nearest to where the world happened to begin, so
/// every other city on the body was a levelled plateau with a mark on
/// the chart and nothing standing on it, and driving to the next town
/// arrived at an empty field.
const TOWNS_BUILT: usize = 8;
/// How far a town may be and still be BUILT, metres.
///
/// The count on its own would drag eight towns across an ocean to keep
/// itself full, which is eight cities' triangles held for a view of
/// water. Two hundred kilometres is about the horizon from the top of
/// the atmosphere and a good deal past what a car can see.
const TOWNS_REACH: f64 = 200_000.0;
/// The world's seed.
const SEED: u32 = 7;
/// How many LOD levels of terrain the harness streams, finest 0.5 m.
///
/// Fourteen, and the four past ten are what closes the gap between the
/// ground and the CHART. A level's box is `2 * HALF * CH` cells across,
/// so ten levels reach 16.4 km from the eye and fourteen reach 262 km,
/// and the coarsest cell goes from 256 m to 4,096 m against a chart texel
/// of 6,136 m on this body.
///
/// The owner asked for a seamless climb and the pictures said why there
/// was not one: at 33 km up, ten levels drew a sharp 32 km island of
/// terrain floating over a chart magnified a hundred times, a 24 fold
/// cliff in detail with nothing in between. At fourteen the same frame is
/// terrain to its edges, and the coarsest cell and the chart's texel are
/// within one and a half of each other, which is a step an eye cannot
/// find. Measured: 282 chunks and 177,897 triangles at 49 km, against 148
/// and 54,625.
const LEVELS: u8 = 14;
/// Frames a second the loop is held to by default. Vsync is the MONITOR's
/// cap and not a cap at all: a scene this cheap to simulate draws at the
/// refresh rate and holds the card at full clock the whole time, which is
/// a hot room for frames nobody asked for. swarm-demo's number and its
/// rule, `--fps 0` lifts it.
const FPS: f64 = 144.0;

/// Radians of look per pixel of mouse.
pub(crate) const LOOK: f32 = 0.0022;

fn main() {
    let args = parse_args();
    if args.bake_atlas {
        world::bake_atlas(&args);
        return;
    }
    if let Some(path) = args.map_png.clone() {
        map::draw_headless(&args, &path);
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
    .init_resource::<Thefts>()
    .init_resource::<lamps::TorchOn>()
    .init_resource::<clock::TimeMenu>()
    .init_resource::<drive::Goal>()
    .init_resource::<fuel::Wallet>()
    .init_resource::<fuel::Jerrycan>()
    .init_resource::<flight_bench::Benchmark>()
    .init_resource::<map::MapView>()
    .init_resource::<map::Markers>()
    .init_resource::<route::Plan>()
    .init_gizmo_group::<map::MapGizmos>()
    .add_systems(
        Startup,
        (
            compute::init_compute,
            spawn_world,
            hud::spawn_hud,
            map::spawn_map,
            flight_bench::setup,
            cull::cull_views,
        )
            .chain(),
    )
    .add_systems(
        PreUpdate,
        flight_bench::clear_input.after(bevy::input::InputSystems),
    )
    .add_systems(Startup, lod_debug::spawn_legend)
    .add_systems(
        PostUpdate,
        flight_bench::before_post.before(bevy::transform::TransformSystems::Propagate),
    )
    .add_systems(
        Last,
        (flight_bench::after_update, flight_bench::count_views),
    );
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
                    // Paired, because Bevy takes twenty systems in one
                    // tuple and this one is at it: a key that fills a
                    // car runs right after the key that boards one, and
                    // the purse is printed right after the walker's line.
                    (board, fuel::refuel).chain(),
                    (walk, fuel::show_purse).chain(),
                    aim_drive,
                    // Paired, for the twenty: a hit is handed out
                    // before the car is driven, so the knock comes off
                    // the speed it arrived at.
                    (ram::ram_cars, drive_car).chain(),
                    fly,
                    flight_bench::drive,
                    planets::activate,
                    rebase_origin,
                    planet_view::recentre,
                    (city::stream::stream_towns, city::detail::stream_tiles).chain(),
                    roads::stream_roads,
                    stream,
                    flight_bench::after_stream,
                    light_lamps,
                    (
                        traffic::drive_traffic,
                        traffic::drive_highway,
                        traffic::spin_wheels,
                        lamps::hold_torch,
                        lamps::light_headlamps,
                    )
                        .chain(),
                    show_cars,
                )
                    .chain(),
                (
                    place_eye,
                    clock::toggle_menu,
                    clock::press_menu,
                    sky::turn_sun,
                    clock::show_menu,
                    dim_lamps,
                    sky::rebake_env,
                    sky::drift_sky,
                    // The driver's HUD and the map, each its own pair or
                    // chain for the twenty: the HUD is read after the
                    // car has been driven and the sun turned, and the
                    // map's mouse before it is drawn.
                    (hud::show_hud, hud::turn_compass).chain(),
                    (
                        map::toggle_map,
                        map::work_map,
                        route::plan_route,
                        map::draw_relief,
                        map::place_relief,
                        map::draw_map,
                        map::show_route,
                        map::press_clear,
                    )
                        .chain(),
                    status::show_status,
                    lod_debug::apply,
                    shot::take_shot,
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
    let world = world::build(&args);
    let (start_eye, start_look) = aim::aim(&world, &args);
    // The sun, worked out while the world is still here to ask: it is a
    // fact about where the WALKER starts, and `start_eye` is wherever the
    // camera was aimed.
    let here = world::start(&world).0;
    let sun = aim::sun_over(here);
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
    let kit = terrain_material(&mut images, &mut materials, &frames, SEA as f32);
    let material = kit.ground.clone();
    let sheet = water_material(&mut waters, SEA);
    say_world(&world, start_eye, &lat, args.levels);
    // The people and the cars on their streets. It is handed the PLANNED
    // towns and turns a crowd out only on the ones standing, which the
    // town streamer moves: a townsman walking a street nobody has laid
    // the buildings of stands on a bare levelled plateau.
    traffic::turn_out(&mut commands, 0, &world, SEED, &mut meshes, &mut standard);
    // The towns are BUILT by `city::stream`, one at a time, following
    // the eye. Nothing is raised here.
    commands.insert_resource(city::stream::Library(Arc::new(buildings::Library::load())));
    let world = Arc::new(world);
    say_roads(&mut commands, &world);
    commands.insert_resource(city::Glazing::new(&mut standard));
    commands.insert_resource(kit);
    commands.init_resource::<world::Fabric>();
    commands.init_resource::<city::stream::Building>();
    let mut planets = planets::Planets::load(world);
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
    status::spawn_status(&mut commands);
    clock::spawn_menu(&mut commands);
    let env = spawn_sky(
        &mut commands,
        &mut meshes,
        &mut skies,
        &mut images,
        eye - body.centre,
        aim::clock_of(&args, sun, here, body),
    );
    spawn_camera(&mut commands, body, &args, eye, look, env, &flight);
    commands.insert_resource(Eye(WorldPos(eye)));
    commands.insert_resource(Ground(body.world.clone(), body.centre));
    commands.insert_resource(planets);
}

/// How the ground and the roads came out: whether the terrain a coarse
/// chunk draws covers the tarmac, how deep a town cuts at its edge, and
/// the roads' own census. Measurements for the log, off the frame.
fn measure_ground(world: &World) {
    if let Some((worst, median, roads)) = roads::ground_over_tarmac(world) {
        info!(
            "the ground a coarse chunk draws stands {worst:.2} m over the tarmac of {roads} roads at its worst and {median:.2} m at its median"
        );
    }
    let (deep, steep, mean) = city::worst_cut(world);
    info!(
        "a town CUTS up to {deep:.0} m at its own edge ({mean:.0} m on the mean), which its skirt ramps at up to {:.0}%",
        steep * 100.0
    );
    roads::report(world);
}

/// The body's road network, and a line saying how much tarmac there is.
/// Every stretch of it is known from the first frame and the ones near
/// the eye are laid as it moves, which is `city::stream`'s own rule for
/// a town.
fn say_roads(commands: &mut Commands, world: &Arc<World>) {
    let network = roads::Network::of(world);
    // How far the LIGHTING reaches out of a town, measured on the road
    // out of the port rather than restated from `road::LIT_NEAR`: what
    // a picture of a lit approach has to be aimed at is where the lamps
    // actually stop, and the first night render of one came back black
    // because the camera stood a hundred metres past them.
    let lit = world.routes.first().map(|r| {
        let open = r.open.iter().position(|o| *o).unwrap_or(0);
        // The first UNLIT piece past it, and not the last lit one
        // anywhere: a road is lit at BOTH ends, so `rposition` measured
        // the whole road and reported 121.6 km of approach.
        let dark = r.lit[open..].iter().position(|l| !*l).unwrap_or(0) + open;
        (
            open,
            dark,
            r.line[open].angle_between(r.line[dark]) * world.planet.radius,
        )
    });
    // Whether the highway JOINS the city it leaves, which is a number
    // and not a thing to squint at a picture for. On a thread of its own,
    // because it is log lines and nothing waits on them: in the frame it
    // was two minutes of every launch before cities grew, and seven after.
    let measured = world.clone();
    std::thread::spawn(move || measure_ground(&measured));
    let (nodes, steps) = network.graph().size();
    info!(
        "{} roads are {} stretches of tarmac with {} gas stations on them, and a graph of {nodes} waypoints and {steps} steps to route over; the ones within {:.0} km of the eye are laid{}",
        world.roads.len(),
        network.len(),
        network.pumps(),
        roads::REACH / 1000.0,
        match lit {
            Some((open, dark, run)) => format!(
                ", and road 0 is lit from its first open piece {open} to piece {dark}, {run:.0} m of approach"
            ),
            None => String::new(),
        }
    );
    commands.insert_resource(network);
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
            // NOUGHT, and `sky::turn_sun` is the one writer of it: how
            // hard the sun burns is a question about what time it is,
            // and it goes out entirely on the night side, which is why
            // a wall at midnight is no longer lit from under the ground.
            illuminance: 0.0,
            shadows_enabled: true,
            ..default()
        },
        cull::sun_layers(),
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
        // The BARE ground: nothing is built yet on the frame the camera
        // is spawned, because `city::stream` raises the first town on
        // the frame after. `world::start` puts the walker on a STREET,
        // so the town arriving under him does not arrive inside him.
        let w = Walker::enter(&world.ground(), &world.bounds, local, d);
        commands.insert_resource(OnFoot(w));
    }
    commands
        .spawn((
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
            // The UI's own camera, said outright: the map's overlay is a
            // second camera on this window, and two cameras with no
            // word on which draws the UI is a warning and a guess.
            IsDefaultUiCamera,
            fly,
        ))
        // The TORCH, a child of the camera so it points wherever the eye
        // does and needs nothing to move it: a light carried by a body is
        // the body's frame, which is this project's own rule for anything
        // standing on a thing that moves.
        .with_child((
            SpotLight {
                intensity: 0.0,
                range: lamps::TORCH_REACH,
                inner_angle: lamps::TORCH_INNER,
                outer_angle: lamps::TORCH_OUTER,
                color: lamps::TORCH_COLOUR,
                shadows_enabled: false,
                ..default()
            },
            Transform::IDENTITY,
            lamps::Torch,
        ));
}

/// Left click takes the mouse, Escape gives it back.
fn grab_mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<clock::TimeMenu>,
    map: Res<map::MapView>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor.single_mut() else {
        return;
    };
    // A panel a player has to PRESS is a panel the cursor has to be
    // free for: a left click on one of its buttons must not also be the
    // click that takes the mouse back off him. The map is the same: a
    // click on it sets a marker.
    if buttons.just_pressed(MouseButton::Left) && !menu.open && !map.open {
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
    thefts: Res<Thefts>,
    mut cam: Query<(&mut Transform, &Fly), With<Camera3d>>,
) {
    let Ok((mut tf, fly)) = cam.single_mut() else {
        return;
    };
    let at = frame.0.local(eye.0);
    // THREE places the eye can be and not two: on foot, at the wheel of
    // a stolen car, or in the air. The first cut had two, so stealing a
    // car (which takes the walker away) put the camera back in the fly
    // rotation and the picture came back with no car in it at all.
    *tf = match (thefts.driving(), walker) {
        (Some(theft), _) => {
            let look = drive::look_at(&theft.car, theft.swing);
            Transform::from_translation(at).looking_to(look.as_vec3(), theft.car.dir.as_vec3())
        }
        (None, Some(w)) => {
            Transform::from_translation(at).looking_to(w.0.look().as_vec3(), w.0.dir.as_vec3())
        }
        (None, None) => Transform::from_translation(at).with_rotation(fly.rotation),
    };
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
