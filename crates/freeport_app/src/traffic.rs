//! The people and the cars on a town's streets, as ENTITIES near the eye.
//!
//! `freeport_core::traffic` says where everybody is at a moment and this
//! draws the few dozen close enough to be looked at. It is the lamps' own
//! rule (`lamps.rs`) on the other kind of thing a city is full of: a town
//! knows about two hundred townsmen and the ECS holds the ones within
//! `REACH`, spawned as the eye comes near and despawned as it leaves.
//!
//! Nothing here integrates. An agent is on rails, so its place is a
//! function of the clock, and an entity that has just been spawned is
//! exactly where it would have been had it existed all along. That is
//! what makes spawning by distance free of any seam: there is no state to
//! carry across the boundary, because there is no state.

use crate::stream::Frame;
use crate::world::Ground;
use crate::Eye;
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use freeport_core::dc::DcMesh;
use freeport_core::figure;
use freeport_core::pos::WorldPos;
use freeport_core::town::{self, lot_frame, Town};
use freeport_core::traffic::{Kind, Traffic};

/// How far a townsman is drawn from, metres, and how many at most of
/// each. Further than a lamp's sixty, because a person walking is a
/// silhouette that reads from much further than the light he stands
/// under, and the count is what bounds the cost rather than the reach.
const REACH: f64 = 95.0;
const MOST_FOLK: usize = 32;
const MOST_CARS: usize = 14;

/// How much nearer somebody already OUT is ranked than he is, so the
/// count does not flicker. An agent MOVES, which is what makes this
/// different from the lamps: `light_lamps` holds its set still by not
/// looking again until the eye has gone four metres, and a townsman
/// walks over the boundary on his own. Without it the thirty second and
/// thirty third nearest swap places every few frames and somebody forty
/// metres off blinks. A quarter, so a newcomer has to be a third nearer
/// to take a place rather than a hair nearer.
const KEEP: f64 = 0.75;

/// How many colours a crowd differs in. Eight is enough that a street
/// does not read as a uniform and few enough that every mesh is built
/// once at startup.
const TINTS: usize = 8;

/// What a figure's own tint can be: a coat, or a car's paint. Linear,
/// because that is what a shader does arithmetic in.
///
/// None of them is near white, which the first render said: at this
/// camera's exposure a coat at 0.72 comes back as a MANNEQUIN, and a
/// street of them reads as a shop window rather than as people. The
/// lightest here is a cream at 0.52 with warmth in it.
const TINT: [[f32; 3]; TINTS] = [
    [0.34, 0.19, 0.14],
    [0.13, 0.18, 0.31],
    [0.52, 0.47, 0.38],
    [0.18, 0.26, 0.19],
    [0.44, 0.13, 0.11],
    [0.11, 0.11, 0.13],
    [0.40, 0.38, 0.36],
    [0.22, 0.31, 0.38],
];

/// What everything else is, indexed by `figure`'s own materials. The two
/// `figure::tinted` names are never read from here.
const PALETTE: [[f32; 3]; figure::KINDS] = [
    [0.58, 0.40, 0.30],
    [0.0, 0.0, 0.0],
    [0.09, 0.08, 0.08],
    [0.0, 0.0, 0.0],
    [0.10, 0.13, 0.16],
    [0.05, 0.05, 0.055],
    [1.00, 0.96, 0.85],
    [0.55, 0.05, 0.04],
];

/// How bright a lamp on a car burns, so a headlight is a headlight after
/// dark rather than a pale box. The same order as a building's lit pane.
const LAMP_NITS: f32 = 900.0;

/// Where the world is this frame: the eye, the origin the render frame
/// is measured from, the body under it and the clock.
///
/// One thing rather than five parameters, which is this project's own
/// rule that `#[allow(clippy::too_many_arguments)]` is the smell of a
/// missing struct. A townsman's place is a function of all of them and
/// of none of them alone.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Here<'w> {
    eye: Res<'w, Eye>,
    frame: Res<'w, Frame>,
    ground: Res<'w, Ground>,
    time: Res<'w, Time>,
    planets: Res<'w, crate::planets::Planets>,
}

/// Which agent an entity is, and which piece of it.
#[derive(Component)]
pub struct Rider {
    /// Which of `Crowds::towns` it belongs to.
    town: usize,
    /// Which of that town's agents.
    agent: usize,
    ride: Ride,
}

/// What the gait does to this entity.
enum Ride {
    /// The whole figure: it takes the world place and the heading.
    Whole,
    /// A leg, on its own pivot, swinging on the walk.
    Limb { phase: f64, at: Vec3 },
}

/// Every built town's traffic, and the meshes it is drawn with.
#[derive(Resource)]
pub struct Crowds {
    /// Which BODY these towns stand on. A crowd belongs to one planet,
    /// and flying to the next one would otherwise put this planet's
    /// townsmen on that one's ground, at that one's radius.
    home: usize,
    /// A town of the world, and everybody out on it.
    towns: Vec<(Town, Traffic)>,
    /// Per tint: the body, then the two legs.
    folk: Vec<[Handle<Mesh>; 3]>,
    /// Per tint: the car, less its lamps.
    cars: Vec<Handle<Mesh>>,
    /// The lamps of a car, which take no tint and are shared.
    lamps: Handle<Mesh>,
    /// Where each leg's hip stands and how far out of phase it swings,
    /// read off the figure once rather than rebuilt at every spawn.
    legs: [(Vec3, f64); 2],
    paint: Handle<StandardMaterial>,
    glow: Handle<StandardMaterial>,
}

impl Crowds {
    /// How many people and cars the built towns turn out in total.
    pub fn count(&self) -> (usize, usize) {
        let of = |k: Kind| {
            self.towns
                .iter()
                .map(|(_, t)| t.agents.iter().filter(|a| a.kind == k).count())
                .sum()
        };
        (of(Kind::Foot), of(Kind::Car))
    }
}

/// Build the crowds of the towns that are BUILT, with a mesh per tint.
///
/// The built towns and no others, because a townsman walking a street
/// nobody has laid the buildings of would be a figure standing on a bare
/// levelled plateau, which is the same gap a far city already has.
/// A figure wears its colour in the VERTEX and not in a material of its
/// own: eight tints times four parts is thirty two meshes built once at
/// startup, against a material per agent spawned and dropped every time
/// somebody walks past.
pub fn turn_out(
    commands: &mut Commands,
    home: usize,
    towns: &[Town],
    seed: u32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let person = figure::person();
    let car = figure::car();
    let lit = |m: u8| m == figure::HEAD_LAMP || m == figure::TAIL_LAMP;
    let crowds = Crowds {
        home,
        towns: towns
            .iter()
            .map(|t| (t.clone(), Traffic::of(t, seed)))
            .collect(),
        folk: (0..TINTS)
            .map(|k| {
                std::array::from_fn(|p| meshes.add(to_mesh(&person.parts[p].mesh, k, |_| true)))
            })
            .collect(),
        cars: (0..TINTS)
            .map(|k| meshes.add(to_mesh(&car.parts[0].mesh, k, |m| !lit(m))))
            .collect(),
        lamps: meshes.add(to_mesh(&car.parts[0].mesh, 0, lit)),
        legs: std::array::from_fn(|k| match person.parts[k + 1].swing {
            figure::Swing::Leg { phase } => (person.parts[k + 1].at.as_vec3(), phase),
            figure::Swing::Still => (Vec3::ZERO, 0.0),
        }),
        paint: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.75,
            ..default()
        }),
        glow: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::rgb(LAMP_NITS, LAMP_NITS * 0.94, LAMP_NITS * 0.8),
            ..default()
        }),
    };
    let (folk, cars) = crowds.count();
    info!(
        "{} towns turn out {folk} on foot and {cars} driving, of which the nearest {} and {} are entities",
        crowds.towns.len(),
        MOST_FOLK,
        MOST_CARS
    );
    commands.insert_resource(crowds);
}

/// One part of a figure as a Bevy mesh, with `figure`'s own materials
/// resolved into vertex colours and the tint filled in.
fn to_mesh(m: &DcMesh, tint: usize, keep: impl Fn(u8) -> bool) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colours = Vec::new();
    for (t, &material) in m.indices.chunks(3).zip(&m.materials) {
        if !keep(material) {
            continue;
        }
        let c = if figure::tinted(material) {
            TINT[tint]
        } else {
            PALETTE[material as usize]
        };
        for &i in t {
            positions.push(m.positions[i as usize]);
            normals.push(m.normals[i as usize]);
            colours.push([c[0], c[1], c[2], 1.0]);
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

/// Where an agent stands in the world, and which way it is going.
fn pose(town: &Town, radius: f64, traffic: &Traffic, agent: usize, time: f64) -> (DVec3, Quat) {
    let spot = traffic.at(&traffic.agents[agent], time);
    let frame = lot_frame(radius, town, spot.at.x, spot.at.y);
    let (sin, cos) = spot.yaw.sin_cos();
    // The figure's own frame: x to its right, y the way it is going, z up.
    // Right is the heading turned a quarter CLOCKWISE, which in a frame
    // with east and north is `east * sin - north * cos`, and the three
    // are right handed, so a box wound out of the mesh stays wound out.
    let fwd = frame.east * cos + frame.north * sin;
    let right = frame.east * sin - frame.north * cos;
    let basis = Mat3::from_cols(right.as_vec3(), fwd.as_vec3(), frame.dir.as_vec3());
    // What it stands ON: a car the carriageway, a person the pavement,
    // which is a kerb higher except where he crosses a street. Asked of
    // the same nine cells the crossing's mesh is paved from, so a foot
    // and the concrete under it cannot disagree.
    let up = town::LIFT
        + match traffic.agents[agent].kind {
            Kind::Foot => traffic.lift(spot.at),
            Kind::Car => 0.0,
        };
    (frame.world(DVec3::Z * up), Quat::from_mat3(&basis))
}

/// Spawn the townsmen within reach of the eye, despawn those out of it,
/// and put every one of them where the clock says it is.
///
/// One system and one query, because an agent's whole state is its index:
/// a leg works its own swing out from the same function its body's place
/// came from rather than reading anything off its parent.
pub fn drive_traffic(
    mut commands: Commands,
    here: Here,
    crowds: Res<Crowds>,
    mut riding: Query<(Entity, &Rider, &mut Transform)>,
) {
    let (eye, frame, ground, time, planets) = (
        &here.eye,
        &here.frame,
        &here.ground,
        &here.time,
        &here.planets,
    );
    // A crowd belongs to ONE body. Off it, everybody goes home rather
    // than being redrawn against another planet's radius and centre.
    if planets.active != crowds.home {
        for (e, rider, _) in &riding {
            if matches!(rider.ride, Ride::Whole) {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let now = time.elapsed_secs_f64();
    let radius = ground.0.planet.radius;
    let centre = ground.1;
    // Who is out already, so the ranking can prefer them and the set
    // does not flicker under people walking across its edge.
    let mut have: Vec<(usize, usize)> = riding
        .iter()
        .filter(|(_, r, _)| matches!(r.ride, Ride::Whole))
        .map(|(_, r, _)| (r.town, r.agent))
        .collect();
    have.sort_unstable();
    let want = near(&crowds, eye, centre, radius, now, &have);
    for (e, rider, mut tf) in &mut riding {
        let key = (rider.town, rider.agent);
        if want.binary_search(&key).is_err() {
            if matches!(rider.ride, Ride::Whole) {
                commands.entity(e).despawn();
            }
            continue;
        }
        let (town, traffic) = &crowds.towns[rider.town];
        match rider.ride {
            Ride::Whole => {
                let (at, turn) = pose(town, radius, traffic, rider.agent, now);
                tf.translation = frame.0.local(WorldPos(centre + at));
                tf.rotation = turn;
            }
            Ride::Limb { phase, at } => {
                let spot = traffic.at(&traffic.agents[rider.agent], now);
                tf.translation = at;
                tf.rotation = Quat::from_rotation_x(figure::gait(spot.along, phase) as f32);
            }
        }
    }
    for key in want {
        if have.binary_search(&key).is_ok() {
            continue;
        }
        spawn(&mut commands, &crowds, frame, centre, radius, key, now);
    }
}

/// Which agents are near enough to be worth an entity: the nearest
/// `MOST_FOLK` and `MOST_CARS` within `REACH`, sorted so a lookup is a
/// binary search.
fn near(
    crowds: &Crowds,
    eye: &Eye,
    centre: DVec3,
    radius: f64,
    now: f64,
    out: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut folk: Vec<(f64, usize, usize)> = Vec::new();
    let mut cars: Vec<(f64, usize, usize)> = Vec::new();
    for (t, (town, traffic)) in crowds.towns.iter().enumerate() {
        // A whole TOWN first, because on foot the eye is inside one of
        // them and every other is a hundred kilometres off: this is the
        // chunk rule (`Planet::around`) at the crowd's own scale.
        let middle = lot_frame(radius, town, 0.0, 0.0).world(DVec3::ZERO) + centre;
        if (middle - eye.0 .0).length() > REACH + town.radius * 2.0 {
            continue;
        }
        // The eye in the TOWN's own metres, so the distance to an agent
        // is two subtractions rather than a frame apiece.
        let here = lot_frame(radius, town, 0.0, 0.0).local(eye.0 .0 - centre);
        for (a, agent) in traffic.agents.iter().enumerate() {
            let spot = traffic.at(agent, now);
            let d = (spot.at.x - here.x).hypot(spot.at.y - here.y);
            if d > REACH {
                continue;
            }
            // Somebody already out ranks NEARER than he is, so he keeps
            // his place until a newcomer is a third closer.
            let rank = if out.binary_search(&(t, a)).is_ok() {
                d * KEEP
            } else {
                d
            };
            match agent.kind {
                Kind::Foot => folk.push((rank, t, a)),
                Kind::Car => cars.push((rank, t, a)),
            }
        }
    }
    let mut out = Vec::with_capacity(MOST_FOLK + MOST_CARS);
    for (list, most) in [(&mut folk, MOST_FOLK), (&mut cars, MOST_CARS)] {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        list.truncate(most);
        out.extend(list.iter().map(|&(_, t, a)| (t, a)));
    }
    out.sort_unstable();
    out
}

/// Put one agent in the world: a car is one entity and a person is three,
/// because his legs turn on their own hips.
fn spawn(
    commands: &mut Commands,
    crowds: &Crowds,
    frame: &Frame,
    centre: DVec3,
    radius: f64,
    (t, a): (usize, usize),
    now: f64,
) {
    let (town, traffic) = &crowds.towns[t];
    let agent = &traffic.agents[a];
    let tint = agent.id as usize % TINTS;
    let (at, turn) = pose(town, radius, traffic, a, now);
    let transform = Transform {
        translation: frame.0.local(WorldPos(centre + at)),
        rotation: turn,
        scale: Vec3::ONE,
    };
    let rider = |ride| Rider {
        town: t,
        agent: a,
        ride,
    };
    match agent.kind {
        Kind::Car => {
            commands
                .spawn((
                    Mesh3d(crowds.cars[tint].clone()),
                    MeshMaterial3d(crowds.paint.clone()),
                    transform,
                    rider(Ride::Whole),
                ))
                .with_child((
                    Mesh3d(crowds.lamps.clone()),
                    MeshMaterial3d(crowds.glow.clone()),
                    Transform::IDENTITY,
                    bevy::light::NotShadowCaster,
                ));
        }
        Kind::Foot => {
            let mut body = commands.spawn((
                Mesh3d(crowds.folk[tint][0].clone()),
                MeshMaterial3d(crowds.paint.clone()),
                transform,
                rider(Ride::Whole),
            ));
            for (k, &(at, phase)) in crowds.legs.iter().enumerate() {
                body.with_child((
                    Mesh3d(crowds.folk[tint][k + 1].clone()),
                    MeshMaterial3d(crowds.paint.clone()),
                    Transform::from_translation(at),
                    rider(Ride::Limb { phase, at }),
                ));
            }
        }
    }
}
