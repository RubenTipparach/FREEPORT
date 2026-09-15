//! freeport_app: the Bevy harness. It draws what `freeport_core` says.
//!
//! A planet ten kilometres across, dual contoured at eleven levels round
//! the eye by the streamer (`stream.rs`) on worker threads, a pad, a wall
//! and a step built at the site the walker starts on, wearing the baked
//! sets (`terrain.rs`), with the core's walker on it (`walk.rs`), a fly
//! camera, a wireframe toggle, and a screenshot flag so a picture can be
//! taken headless under Xvfb and lavapipe. The eye is a world position in
//! `f64` and the camera is placed from it through the floating origin.
//!
//! ```text
//! freeport_app [--wire] [--fly] [--eye x,y,z] [--look x,y,z] [--levels N]
//!              [--shot out.png] [--frames N] [--sculpt block|slab|pillar|ball|ramp|pad|room|door|window]
//! ```
//!
//! Left click takes the mouse, Escape gives it back. On foot: WASD, Shift
//! runs, Space jumps. Flying: WASD and Q E, Shift is faster. F swaps the
//! two, Tab toggles the wireframe. `--eye` is where to start, metres from
//! the planet's centre (on foot, the spot under it) and `--look` what to
//! face; both default to the site.

mod edit;
mod lamps;
mod stream;
mod terrain;
mod tiers;
mod walk;
mod water;

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
use edit::{build, Builder};
use freeport_core::field::{Block, Built, Density, Planet, Structure, STREET};
use freeport_core::lattice::Lattice;
use freeport_core::pos::WorldPos;
use freeport_core::recipe::{Building, Recipe};
use freeport_core::town::{self, lot_frame, Town};
use freeport_core::walker::{Bounds, Walker};
use freeport_core::water::{Sea, Water};
use lamps::light_lamps;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use stream::{rebase_origin, stream, Frame, Streamer};
use terrain::{terrain_material, TerrainMaterial, TerrainPlugin};
use walk::{toggle_walk, walk, OnFoot};
use water::{water_material, WaterMaterial, WaterPlugin};

/// The planet: five kilometres of radius, so ten across.
const RADIUS: f64 = 5_000.0;
/// The sea's level, metres under the mean radius: with a hundred and sixty
/// of relief, a little under half the surface is under it.
const SEA: f64 = RADIUS - 12.0;
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
/// The hex tier: metres a tile, how many tiles the disc reaches, and how
/// deep a column's skirt hangs. The skirt has to cover the step between
/// two columns and the step from the rim column to the level of detail
/// tier under it, and the relief's steepest is well under a metre a tile.
const HEX_TILE: f64 = 1.0;
const HEX_SPAN: u32 = 48;
const HEX_SKIRT: f64 = 3.0;
/// The far tier: Planet-LOD's quality knob, and how many pieces a leaf's
/// edge is cut into on the way past. The detail on the ground is the
/// product, so this is ratio 24 for the cost of selecting at 6.
const LOD_RATIO: f64 = 6.0;
const LOD_SUB: u32 = 4;

/// Radians of look per pixel of mouse.
pub(crate) const LOOK: f32 = 0.0022;
/// Flying: metres a second, and the factor Shift puts on it.
const SPEED: f64 = 6.0;
const SPRINT: f64 = 8.0;

/// What the command line asked for.
#[derive(Resource, Clone, Debug)]
pub(crate) struct Args {
    wire: bool,
    fly: bool,
    eye: Option<DVec3>,
    look: Option<DVec3>,
    levels: u8,
    shot: Option<String>,
    frames: u32,
    /// A shape the builder places at the crosshair once the first load has
    /// settled, so a headless run can photograph an edit and the chunks it
    /// remade.
    sculpt: Option<String>,
    /// Draw the hex tiers: a disc of Goldberg columns round the eye and
    /// Planet-LOD past it, both made in the vertex stage. It is the
    /// DEFAULT, because the hex world is what this harness is; `--chunks`
    /// is how the dual contoured one is asked for.
    tiers: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        wire: false,
        fly: false,
        eye: None,
        look: None,
        levels: LEVELS,
        shot: None,
        frames: 30,
        sculpt: None,
        tiers: true,
    };
    let mut it = std::env::args().skip(1);
    let vec3 = |s: &str| -> Option<DVec3> {
        let v: Vec<f64> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| DVec3::new(v[0], v[1], v[2]))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wire" => args.wire = true,
            "--fly" => args.fly = true,
            "--eye" => args.eye = it.next().and_then(|v| vec3(&v)),
            "--look" => args.look = it.next().and_then(|v| vec3(&v)),
            "--levels" => {
                args.levels = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(LEVELS)
                    .clamp(1, 16)
            }
            "--shot" => args.shot = it.next(),
            "--frames" => args.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(30),
            "--sculpt" => args.sculpt = it.next(),
            // The tiers are flown over: the walker stands on the dual
            // contoured field, and a hex column's ground is a question
            // `freeport_core::walker` has not been asked yet.
            "--tiers" => args.tiers = true,
            // The dual contoured world: chunks, a sea of its own, the
            // towns and the builder, and the walker on foot in them.
            "--chunks" => args.tiers = false,
            other => warn!("unknown argument {other}"),
        }
    }
    // The hex world is flown over: a hex column's ground is a question
    // `freeport_core::walker` has not been asked yet, so there is nothing
    // for the walker to stand on that agrees with the picture.
    if args.tiers {
        args.fly = true;
    }
    args
}

fn main() {
    let args = parse_args();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            WireframePlugin::default(),
            TerrainPlugin,
            WaterPlugin,
            tiers::TiersPlugin,
        ))
        .insert_resource(WireframeConfig {
            global: args.wire,
            default_color: Color::srgb(0.1, 0.1, 0.12),
        })
        .insert_resource(ClearColor(Color::srgb(0.55, 0.72, 0.92)))
        .insert_resource(args)
        .init_resource::<Eye>()
        .init_resource::<Frame>()
        .init_resource::<Status>()
        .init_resource::<Builder>()
        .add_systems(
            Startup,
            (spawn_world, tiers::spawn_world.run_if(on_tiers)).chain(),
        )
        .add_systems(
            Update,
            (
                grab_mouse,
                toggle_walk,
                walk,
                fly,
                build.run_if(not(on_tiers)),
                rebase_origin,
                stream.run_if(not(on_tiers)),
                tiers::feed_tiers.run_if(on_tiers),
                light_lamps.run_if(not(on_tiers)),
                place_eye,
                show_status,
                toggle_wireframe,
                take_shot,
            )
                .chain(),
        )
        .run();
}

/// Whether the harness draws the hex tiers rather than the dual contoured
/// chunks: the run condition on every system that belongs to one or the
/// other.
fn on_tiers(args: Res<Args>) -> bool {
    args.tiers
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

/// A town's structures, contiguous in `World::structures`, and the box
/// round all of them, so a chunk far from every town tests one box a town
/// and never a structure. Edits are pushed after the last group and are
/// tested one by one.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub lo: DVec3,
    pub hi: DVec3,
    pub range: std::ops::Range<usize>,
}

/// The status line's parts.
#[derive(Resource, Default)]
pub(crate) struct Status {
    pub walker: String,
    pub build: String,
}

/// The line of text that says where the walker stands.
#[derive(Component)]
struct Stat;

/// The field the harness stands on: the planet, what is built on it, and
/// where the walker may look for the ground. Shared with the workers, and
/// cloned whole for an edit, so a worker mid job keeps the world it had.
#[derive(Clone)]
pub(crate) struct World {
    pub planet: Planet,
    pub blocks: Vec<Block>,
    /// Every building and every piece of street on the planet.
    pub structures: Vec<Structure>,
    /// The structures a town at a time, with a box round each town's.
    pub groups: Vec<Group>,
    /// Every lamp in them, in the world frame, with its reach.
    pub lamps: Vec<(DVec3, f64)>,
    pub towns: Vec<Town>,
    pub bounds: Bounds,
    pub sea: Sea,
    /// Cuts made in the dry, which the sea never enters.
    pub dry: Vec<Block>,
}

impl World {
    /// The field inside a box, contoured on `cell`: the ground and every
    /// structure reaching into the box.
    pub fn field_in(&self, lo: DVec3, hi: DVec3, cell: f64) -> Built<'_> {
        let mut structures = Vec::new();
        let mut grouped = 0;
        for g in &self.groups {
            grouped = grouped.max(g.range.end);
            if g.lo.cmple(hi).all() && g.hi.cmpge(lo).all() {
                let town = &self.structures[g.range.clone()];
                structures.extend(town.iter().filter(|st| st.meets(lo, hi)));
            }
        }
        let loose = &self.structures[grouped.min(self.structures.len())..];
        structures.extend(loose.iter().filter(|st| st.meets(lo, hi)));
        Built {
            ground: &self.planet,
            blocks: self.blocks.clone(),
            structures,
            cell,
        }
    }

    /// The field within `reach` of a point, at full detail: what a walker
    /// stands on.
    pub fn field_near(&self, p: DVec3, reach: f64) -> Built<'_> {
        self.field_in(p - DVec3::splat(reach), p + DVec3::splat(reach), FINE)
    }

    /// The sea on that ground.
    pub fn water<'a>(&'a self, ground: &'a dyn Density) -> Water<'a> {
        Water {
            sea: self.sea,
            ground,
            dry: self.dry.clone(),
        }
    }
}

#[derive(Resource)]
pub(crate) struct Ground(pub Arc<World>);

/// The recipes in `assets/buildings`, by name.
fn recipes() -> HashMap<String, Recipe> {
    let mut out = HashMap::new();
    let Some(dir) = terrain::assets_dir().map(|d| d.join("buildings")) else {
        warn!("no assets folder: no recipes, so no buildings");
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        warn!("no recipes in {}", dir.display());
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| Recipe::parse(&t))
            {
                Ok(r) => {
                    out.insert(r.name.clone(), r);
                }
                Err(e) => warn!("{}: {e}", path.display()),
            }
        }
    }
    out
}

/// The planet, its towns and everything built in them. `towns` is nought
/// for the hex tiers, which draw the relief alone: `field.wgsl` has no
/// sites in it yet, so a town's levelled plateau would be in the walker's
/// field and not in the picture.
fn world(towns: usize) -> World {
    let t0 = Instant::now();
    let mut planet = Planet {
        radius: RADIUS,
        relief: 160.0,
        lumps: 10.0,
        octaves: 10,
        overhang: 3.0,
        ledge: 12.0,
        seed: SEED,
        sites: vec![],
    };
    let towns = town::plan(&planet, SEA, TOWN_RADIUS, towns, SEED);
    planet.sites = towns.iter().map(town::site_of).collect();
    let planned = t0.elapsed();
    let recipes = recipes();
    let mut structures: Vec<Structure> = Vec::new();
    let mut groups = Vec::new();
    let mut buildings = 0;
    for t in &towns {
        let start = structures.len();
        for lot in &t.lots {
            let Some(recipe) = recipes.get(lot.recipe).or_else(|| recipes.get("house")) else {
                continue;
            };
            let storeys = lot.storeys.clamp(recipe.storeys[0], recipe.storeys[1]);
            let building = recipe.compile(storeys, SEED ^ lot.id);
            structures.push(Structure::new(lot_frame(RADIUS, t, lot.x, lot.z), building));
            buildings += 1;
        }
        for piece in &t.pieces {
            let slab = Building::slab(
                DVec3::new(0.0, 0.0, -0.05),
                DVec3::new(piece.w, piece.d, 0.6),
                STREET,
                0.0,
            );
            structures.push(Structure::new(lot_frame(RADIUS, t, piece.x, piece.z), slab));
        }
        let town = &structures[start..];
        groups.push(Group {
            lo: town.iter().fold(DVec3::INFINITY, |lo, st| lo.min(st.lo)),
            hi: town
                .iter()
                .fold(DVec3::NEG_INFINITY, |hi, st| hi.max(st.hi)),
            range: start..structures.len(),
        });
    }
    let lamps: Vec<(DVec3, f64)> = structures.iter().flat_map(Structure::lamps).collect();
    if let (Some(port), Some(lot)) = (towns.first(), towns.first().and_then(|t| t.lots.first())) {
        if let Some(recipe) = recipes.get(lot.recipe) {
            let f = lot_frame(RADIUS, port, lot.x, lot.z);
            let outside = f.world(DVec3::new(
                recipe.door,
                -recipe.footprint[1] / 2.0 - 2.5,
                1.7,
            ));
            let inside = f.world(DVec3::new(recipe.door, 0.0, 1.7));
            info!(
                "the port's first lot is a {} of {} storeys; its door from {:.2} looking at {:.2}",
                lot.recipe, lot.storeys, outside, inside
            );
        }
    }
    info!(
        "{} towns planned in {:.0} ms, {} buildings from {} recipes and {} pieces of street built in {:.0} ms, {} lamps",
        towns.len(),
        planned.as_secs_f64() * 1000.0,
        buildings,
        recipes.len(),
        structures.len() - buildings,
        (t0.elapsed() - planned).as_secs_f64() * 1000.0,
        lamps.len()
    );
    let (floor, roof) = planet.band();
    let bounds = Bounds {
        radius: RADIUS,
        floor: floor - 2.0,
        top: roof + 40.0,
        sea: 0.0,
    };
    World {
        planet,
        blocks: Vec::new(),
        structures,
        groups,
        lamps,
        towns,
        bounds,
        sea: Sea { radius: SEA },
        dry: Vec::new(),
    }
}

/// Where a walker starts: on a street of the port, facing the middle of
/// town, or on the pole if there is no town.
fn start(world: &World) -> (DVec3, DVec3) {
    let Some(port) = world.towns.first() else {
        let top = town::surface_radius(&world.planet, DVec3::Y);
        return (DVec3::new(0.5, top, -6.5), DVec3::new(0.0, top + 0.8, 0.0));
    };
    let x = -town::BLOCK / 2.0 - town::STREET / 2.0;
    let f = lot_frame(RADIUS, port, x, -port.radius * 0.6);
    (
        f.world(DVec3::new(0.0, 0.0, 1.7)),
        f.world(DVec3::new(0.0, 10.0, 1.7)),
    )
}

/// The nearest point of the shore to the eye, on rings of directions out
/// from it, so a picture can be aimed at the sea.
fn shore(world: &World, eye: DVec3) -> Option<DVec3> {
    let up = eye.normalize_or(DVec3::Y);
    let (east, north) = town::frame_at(up);
    for ring in 1..400 {
        let a = ring as f64 * 25.0 / RADIUS;
        for k in 0..(ring * 6) {
            let b = k as f64 / (ring * 6) as f64 * std::f64::consts::TAU;
            let dir = (up * a.cos() + (east * b.cos() + north * b.sin()) * a.sin()).normalize();
            if town::surface_radius(&world.planet, dir) < world.sea.radius {
                return Some(dir * world.sea.radius);
            }
        }
    }
    None
}

fn spawn_world(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    args: Res<Args>,
) {
    let world = world(if args.tiers { 0 } else { TOWNS });
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
    info!(
        "planet of {} m, the sea at {} m, {} levels of {} m to {} m cells, the eye at {:.0}, the shore {:.0} m off at {:.0}",
        RADIUS,
        SEA,
        args.levels,
        lat.cell(0),
        lat.cell(args.levels - 1),
        eye,
        shore(&world, eye).map_or(f64::NAN, |s| (s - eye).length()),
        shore(&world, eye).unwrap_or(DVec3::NAN)
    );
    if args.tiers {
        commands.insert_resource(tiers::Tiers {
            radius: RADIUS,
            sea: SEA,
            relief: 160.0,
            lumps: 10.0,
            octaves: 10,
            seed: SEED,
            tile: HEX_TILE,
            span: HEX_SPAN,
            skirt: HEX_SKIRT,
            ratio: LOD_RATIO,
            sub: LOD_SUB,
        });
    } else {
        commands.insert_resource(Streamer::new(lat, eye, args.levels, material, sheet));
    }
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
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 90.0,
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
    spawn_camera(&mut commands, &world, &args, eye, look);
    commands.insert_resource(Eye(WorldPos(eye)));
    commands.insert_resource(Ground(Arc::new(world)));
}

/// The camera, flying from `eye` toward `look`, or on foot at the spot
/// under `eye` facing `look`.
fn spawn_camera(commands: &mut Commands, world: &World, args: &Args, eye: DVec3, look: DVec3) {
    let d = (look - eye).normalize_or(DVec3::NEG_Z);
    let fly = Fly {
        yaw: (-d.x as f32).atan2(-d.z as f32),
        pitch: (d.y as f32).clamp(-1.0, 1.0).asin(),
        at: eye,
    };
    if !args.fly {
        let w = Walker::enter(&world.field_near(eye, 8.0), &world.bounds, eye, d);
        commands.insert_resource(OnFoot(w));
    }
    commands.spawn((
        Camera3d {
            screen_space_specular_transmission_steps: 1,
            ..default()
        },
        DepthPrepass,
        Exposure { ev100: 10.5 },
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
    tiers: Option<Res<tiers::Tiers>>,
    mut text: Query<&mut Text, With<Stat>>,
) {
    let what = match (&streamer, &tiers) {
        (Some(s), _) => s.status(),
        (_, Some(t)) => format!(
            "hex {:.2} m tiles in a disc of {:.0} m, Planet-LOD at ratio {} cut {} ways",
            t.grid().spacing(t.radius),
            t.disc() * t.radius,
            t.ratio,
            t.sub
        ),
        _ => String::new(),
    };
    if let Ok(mut text) = text.single_mut() {
        text.0 = format!(
            "{}   |   {}{}   |   F fly, B build, Tab wire, Esc mouse",
            status.walker,
            if status.build.is_empty() {
                String::new()
            } else {
                format!("{}   |   ", status.build)
            },
            what
        );
    }
}

fn toggle_wireframe(keys: Res<ButtonInput<KeyCode>>, mut config: ResMut<WireframeConfig>) {
    if keys.just_pressed(KeyCode::Tab) {
        config.global = !config.global;
    }
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
    // A scripted edit is placed the frame after the first load settles, so
    // the picture waits for it and for the chunks it remade. The tiers
    // have nothing to settle: what they draw is a function of where the
    // eye is on the frame it is drawn.
    let ready = match &streamer {
        Some(s) => {
            let edited = args.sculpt.is_none() || s.edits() > 0;
            (s.idle() && edited) || *frame >= args.frames * 10
        }
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
