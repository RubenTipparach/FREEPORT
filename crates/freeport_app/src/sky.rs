//! The sky, and the fog under it, off one march.
//!
//! `freeport_core::atmos` is tenebris's single scatter march in the core.
//! This draws it two ways and they cannot disagree, because they are the
//! same function: the DOME is `atmos.wgsl`, a sphere round the eye whose
//! every fragment is a view ray marched on the GPU; the FOG is
//! `atmos::horizon` on the CPU, this air's own sky at the horizon, handed
//! to the ground and the sea as a colour and a density so they fade into
//! the sky they stand under. Tenebris samples its fog off its own sky
//! every frame for exactly this reason, and says so.

mod environment;
pub(crate) use environment::StaticEnvironment;

use crate::terrain::TerrainMaterial;
use crate::water::WaterMaterial;
use bevy::asset::embedded_asset;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::math::DVec3;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use freeport_core::atmos::{self, Air};

/// The dome MESH's own radius, metres. `dome_radius` is what it is drawn
/// at, and this is only the size it is built at, so a scale of one is the
/// common case on a planet of this size.
const DOME: f32 = 500_000.0;

/// How far out the dome is drawn, metres: further than anything else in
/// the scene, which is the far limb of the shell, `|eye| + top`. A FIXED
/// radius was enough while the planet was ten kilometres across and is
/// not at a thousand: from four radii up the eye is three thousand
/// kilometres off a dome five hundred wide, so the dome stood IN FRONT of
/// the planet and painted it out, a pale blue disc with no ground in it
/// at all. Twice over is the margin, and it is free: the dome's colour is
/// a DIRECTION, so nothing about it changes when it grows.
fn dome_radius(here: DVec3, air: &Air) -> f32 {
    ((here.length() + air.top) * 2.0) as f32
}

/// What the dome's shader is handed: the planet, the sun and the air.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Sky {
    /// The planet's centre in the render frame, w: the dome's radius.
    #[uniform(100)]
    pub centre: Vec4,
    /// Where the sun is, as a direction.
    #[uniform(100)]
    pub sun: Vec4,
    /// x: the ground's radius, y: the shell's, z: the lowest the ground
    /// reaches, which is what stops a view ray going down.
    #[uniform(100)]
    pub shell: Vec4,
    /// x: the scale height, y: rayleigh, z: mie, w: mie's g.
    #[uniform(100)]
    pub coef: Vec4,
    /// rgb: the wavelength ratios, w: the sun's intensity.
    #[uniform(100)]
    pub waves: Vec4,
    /// rgb: the dusk tint, w: its strength.
    #[uniform(100)]
    pub tint: Vec4,
    #[uniform(100)]
    pub glow: Vec4,
    #[uniform(100)]
    pub band: Vec4,
}

impl Sky {
    /// The dome's lanes for an air and a sun.
    pub fn of(air: &Air, sun: DVec3) -> Sky {
        Sky {
            centre: Vec4::new(0.0, 0.0, 0.0, DOME),
            sun: sun.as_vec3().extend(atmos::NITS as f32),
            shell: Vec4::new(air.ground as f32, air.top as f32, air.floor as f32, 0.0),
            coef: Vec4::new(
                air.scale_height as f32,
                air.rayleigh as f32,
                air.mie as f32,
                air.mie_g as f32,
            ),
            waves: air.waves.as_vec3().extend(air.sun as f32),
            tint: air.tint.as_vec3().extend(air.sunset as f32),
            glow: air.glow.as_vec3().extend(0.0),
            band: air.band.as_vec3().extend(0.0),
        }
    }
}

impl Material for Sky {
    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/sky.wgsl".into()
    }

    /// The dome is the sky where nothing else was drawn, so it blends
    /// over what is behind it (space) and writes no depth of its own.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Seen from the inside, so the front faces are the ones to drop.
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// The air and the sun this world stands under, so one number is read by
/// the dome, by the fog and by the light that casts the shadows.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Weather {
    pub air: Air,
    /// The sea's radius, which is where the ground fog is thickest.
    pub sea: f64,
    /// Where the sun is NOW, as a direction in the world frame. It is
    /// derived from `noon` and `now` once a frame by `turn_sun` and is
    /// what the light, the dome, the fog, the sea, the lamps and the
    /// bodies drawn from far off all read, so they cannot point six ways.
    pub sun: DVec3,
    /// Where the sun stood when this body's clock read nought.
    pub noon: DVec3,
    /// Seconds into the body's own day when the app's own clock read
    /// nought, which is what `--hour` sets.
    pub start: f64,
    /// Seconds into the body's own day. It only ever grows; `day::hour`
    /// is what turns it into a place on the dial.
    pub now: f64,
    /// How long one of this body's days is, seconds.
    pub day: f64,
    /// Where the EYE is on this body, as a direction: which of its hours
    /// the lamps are lit by, and what `turn_sun` measures the clock at.
    pub here: DVec3,
}

impl Weather {
    /// What o'clock it is where the eye stands.
    pub fn oclock(&self) -> f64 {
        freeport_core::day::oclock(self.sun, self.here)
    }

    /// How hard the lamps are burning where the eye stands, one at night
    /// and nought by day. The CPU's answer, read by `lamps.rs`; the
    /// fragment shaders transcribe `day::daylight` and get the same one
    /// per fragment.
    pub fn lamplight(&self) -> f64 {
        freeport_core::day::lamplight(self.sun, self.here)
    }
}

/// A handle held so `atmos.wgsl` is LOADED and not merely registered: an
/// import nothing has asked for is a pipeline Bevy retries in silence.
#[derive(Resource)]
struct Lib(#[allow(dead_code)] Handle<Shader>);

/// The dome's own entity, which is moved onto the eye every frame.
#[derive(Component)]
pub struct Dome;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "atmos.wgsl");
        embedded_asset!(app, "sky.wgsl");
        let lib = app
            .world()
            .resource::<AssetServer>()
            .load("embedded://freeport_app/atmos.wgsl");
        app.insert_resource(Lib(lib));
        app.init_resource::<Baking>();
        app.add_plugins((
            MaterialPlugin::<Sky>::default(),
            environment::StaticEnvironmentPlugin,
        ));
    }
}

/// The dome, once: a sphere round the eye, never culled, never moved by
/// the origin (it is moved onto the camera instead).
pub fn spawn_dome(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    skies: &mut Assets<Sky>,
    weather: &Weather,
) {
    let ico = Sphere::new(DOME)
        .mesh()
        .ico(3)
        .expect("three subdivisions is inside the icosphere's own limit");
    let mesh = meshes.add(ico);
    let material = skies.add(Sky::of(&weather.air, weather.sun));
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        NoFrustumCulling,
        Dome,
    ));
    // The fog as the DISTANCE it fades over rather than as its density:
    // on a thousand kilometre planet the density is seven millionths a
    // metre and four decimals of it print as nought, which is a line that
    // says nothing exactly where the number moved.
    info!(
        "sky: air from {:.0} m to {:.0} m, the sun at {:.2} ({:.2} o'clock, a day is {:.0} min), fog e folding over {:.0} m and gone by {:.0} m up",
        weather.air.ground,
        weather.air.top,
        weather.sun,
        weather.oclock(),
        weather.day / 60.0,
        1.0 / weather.air.fog.max(f64::MIN_POSITIVE),
        weather.air.fog_height
    );
}

/// What the sun is worth at its own noon, lux. It is scaled down to
/// NOUGHT across the terminator by `turn_sun`, which is the one writer
/// of it: `spawn_light` spawns the light dark and this decides how hard
/// it burns, the same split `light_lamps` and `dim_lamps` keep.
pub const SUN_LUX: f32 = 8_000.0;

/// How far the sun may TURN before the sky that lights the world is
/// baked again, radians. Half a degree is the sun's own width, which is
/// the finest step there is any point taking: what the cubemap feeds is
/// the ambient, a cosine average over a whole hemisphere, so it cannot
/// hold a feature narrower than the source that made it. At four hours
/// to a day the sun covers this in twenty seconds of play, and a bake is
/// four to twenty milliseconds on a thread of its own, which is a
/// thousandth of a core.
const RE_BAKE: f64 = 0.0087;

/// The sky being baked for a sun that has moved, on a thread of its own.
///
/// It is a THREAD and not the frame's own work because a bake is up to
/// twenty milliseconds and a frame is sixteen: done in line it would be a
/// visible hitch every twenty seconds, which is a worse picture than the
/// stale ambient it is there to replace. A `JoinHandle` and not a channel
/// because there is exactly ONE answer and no queue: `is_finished` is the
/// poll and `join` on a finished thread returns without waiting, so this
/// system never blocks the frame it runs on.
#[derive(Resource, Default)]
pub struct Baking {
    /// The sun the cubemap on the camera was last baked FOR, which is
    /// what says whether another bake is owed. It is written when the
    /// worker is started rather than when it lands, so a slow bake is
    /// never started twice.
    asked: DVec3,
    waiting: Option<std::thread::JoinHandle<Image>>,
}

/// The sun, turned. `Weather::now` is the body's own clock and everything
/// that reads the sun reads this one number: the light that casts the
/// shadows, the dome, the fog, the sea, the lamps and the bodies drawn
/// from far off.
///
/// The BODY is held still and the sky turns, which is what `day.rs` says
/// and why: every direction this game reasons about is written on a
/// sphere that never moves, so spinning the planet would mean moving
/// every chunk, every town and every lamp in the world once a frame for
/// a picture identical to turning one vector.
///
/// The clock is `Time::elapsed_secs_f64` and an offset rather than a
/// delta summed frame by frame, which is the SAME clock `traffic.rs`
/// puts its townsmen on: two clocks is how a sun and the people under it
/// come to disagree about what time it is.
pub fn turn_sun(
    time: Res<Time>,
    eye: Res<crate::Eye>,
    ground: Res<crate::Ground>,
    mut weather: ResMut<Weather>,
    mut light: Query<(&mut Transform, &mut DirectionalLight)>,
) {
    weather.now = weather.start + time.elapsed_secs_f64();
    weather.sun = freeport_core::day::sun_at(weather.noon, weather.now, weather.day);
    weather.here = (eye.0 .0 - ground.1).normalize_or(DVec3::Y);
    let sun = weather.sun.as_vec3();
    // The light shines the way the sun is NOT: Bevy's forward is negative
    // z and a directional light travels along it. The up is the axis the
    // sun is LEAST along, because a `looking_to` whose up is parallel to
    // its direction has no frame to build and the sun passes over the
    // pole twice a year on any body with a tilt.
    let up = if sun.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    // AND IT GOES OUT ON THE NIGHT SIDE. A directional light shines on
    // every surface whose normal faces it, and nothing in a cascade a
    // few hundred metres deep can put a PLANET in the way: at midnight
    // the sun stands under the ground and every wall facing it was lit
    // from below, which is sunlight shining up through the world. The
    // owner saw it and named where the answer is.
    //
    // It is tenebris's, and it is one line there too (`hex.vs.glsl`):
    // `smoothstep(term_lo, term_hi, dot(radial, sun))` scales the sun's
    // own diffuse, so which half of a planet is in its own night is
    // decided on the RADIAL and not on the surface normal. That is
    // `freeport_core::day::daylight`, which this crate already carries
    // and which `distant.wgsl`, `water.wgsl` and `terrain.wgsl` already
    // transcribe; the sun was the one thing not reading it.
    //
    // At the EYE's own radial, because Bevy's light loop is inside
    // `apply_pbr_lighting` and there is nowhere to scale one light per
    // fragment without writing the loop again. On the ground that is
    // exact to a tenth of a degree, which is what a 1.8 km horizon
    // subtends; from the air near the terminator it is one answer for a
    // scene that spans several degrees of it, and from orbit the
    // impostor does the same rule per fragment off its own chart.
    let lux = SUN_LUX * freeport_core::day::daylight(weather.sun, weather.here) as f32;
    for (mut tf, mut lamp) in &mut light {
        *tf = Transform::from_translation(Vec3::ZERO).looking_to(-sun, up);
        lamp.illuminance = lux;
    }
}

/// The sky baked into the cubemap again once the sun has moved, on a
/// worker, and swapped in at the same handle when it lands.
///
/// Replacing the IMAGE is what regenerates the filtered light:
/// `StaticEnvironment` caches Bevy's own filtering on the source
/// texture's id (`sky/environment.rs`), so a new image is a new texture
/// is a new filter, and nothing here has to know how that is done.
pub fn rebake_env(
    weather: Res<Weather>,
    eye: Res<crate::Eye>,
    ground: Res<crate::Ground>,
    mut baking: ResMut<Baking>,
    mut images: ResMut<Assets<Image>>,
    lights: Query<&bevy::light::GeneratedEnvironmentMapLight>,
) {
    let Ok(light) = lights.single() else {
        return;
    };
    if baking.waiting.as_ref().is_some_and(|j| !j.is_finished()) {
        return;
    }
    if let Some(job) = baking.waiting.take() {
        // A worker that panicked leaves the sky it baked LAST, which is a
        // stale ambient rather than a black one: the picture is wrong by
        // however far the sun has gone since, and never a hole.
        if let Ok(image) = job.join() {
            if let Err(e) = images.insert(&light.environment_map, image) {
                warn!("the sky could not be baked again: {e}");
            }
        }
        return;
    }
    if weather.sun.angle_between(baking.asked) < RE_BAKE {
        return;
    }
    let (air, sun, here) = (weather.air, weather.sun, eye.0 .0 - ground.1);
    baking.asked = sun;
    baking.waiting = std::thread::spawn(move || bake_env(&air, sun, here)).into();
}

/// The materials the sky is painted onto: the ground, the sea and the
/// bodies drawn from far off. One system hands the first two the same fog
/// colour and the same density, because they stand under one sky, and the
/// third the SUN, because which half of a body is in its own night is the
/// same fact the dome is drawing.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Painted<'w> {
    pub ground: ResMut<'w, Assets<TerrainMaterial>>,
    pub water: ResMut<'w, Assets<WaterMaterial>>,
    pub distant: ResMut<'w, Assets<crate::distant::DistantMaterial>>,
}

/// Every frame: the dome onto the eye, the planet's centre and the sun
/// into it, and this air's own horizon into everything that fades with
/// distance.
pub fn drift_sky(
    eye: Res<crate::Eye>,
    frame: Res<crate::stream::Frame>,
    weather: Res<Weather>,
    ground: Res<crate::Ground>,
    mut skies: ResMut<Assets<Sky>>,
    mut dome: Query<&mut Transform, With<Dome>>,
    mut painted: Painted,
) {
    let here = eye.0 .0 - ground.1;
    let centre = frame.0.local(freeport_core::pos::WorldPos(ground.1));
    let at = frame.0.local(eye.0);
    let reach = dome_radius(here, &weather.air);
    for mut tf in &mut dome {
        tf.translation = at;
        tf.scale = Vec3::splat(reach / DOME);
    }
    let ids: Vec<_> = skies.ids().collect();
    for id in ids {
        if let Some(sky) = skies.get_mut(id) {
            *sky = Sky::of(&weather.air, weather.sun);
            sky.centre = centre.extend(reach);
            sky.sun = weather.sun.as_vec3().extend(atmos::NITS as f32);
        }
    }
    // The fog IS the sky, sampled at the horizon by the same march, so
    // the two cannot drift apart: a hillside a hundred metres off fades
    // into the colour the dome is painting right above it.
    // In candela, as the dome's is, because the shaders take it through
    // the camera's exposure beside everything else they draw.
    let colour = (atmos::horizon(&weather.air, here, weather.sun) * atmos::NITS).as_vec3();
    let fog = colour.extend(atmos::fog_density(&weather.air, here) as f32);
    // The ground fog's own shape, measured from the sea rather than from
    // the mean radius, because that is where air pools.
    let haze = Vec4::new(
        weather.air.pool as f32,
        weather.air.pooled as f32,
        weather.sea as f32,
        0.0,
    );
    let ids: Vec<_> = painted.ground.ids().collect();
    for id in ids {
        if let Some(m) = painted.ground.get_mut(id) {
            m.extension.fog = fog;
            m.extension.haze = haze;
            // The SUN, so a street lamp and a lit pane burn at night and
            // not at noon. The w is how hard they burn and is the
            // material's, not the weather's, so it is read back rather
            // than written over, which is what the sea and the bodies
            // drawn from far off already do with theirs.
            m.extension.sun = weather.sun.as_vec3().extend(m.extension.sun.w);
        }
    }
    let ids: Vec<_> = painted.water.ids().collect();
    for id in ids {
        if let Some(m) = painted.water.get_mut(id) {
            m.extension.fog = fog;
            m.extension.haze = haze;
            // The SUN, so the sheet knows which side of the terminator a
            // piece of sea is on: its body takes the night floor there
            // and its reflection takes the sky the dome is painting.
            // The w is the floor itself and is the material's, not the
            // weather's, so it is read back rather than written over,
            // which is what `distant` already does with its lamps.
            m.extension.sun = weather.sun.as_vec3().extend(m.extension.sun.w);
        }
    }
    // The SUN into every body drawn from far off, so the half of one that
    // is in its own night is dark and its cities are lit. The w is the
    // lamps' own strength and is the material's, not the weather's, so it
    // is read back rather than written over.
    let ids: Vec<_> = painted.distant.ids().collect();
    for id in ids {
        if let Some(m) = painted.distant.get_mut(id) {
            m.extension.sun = weather.sun.as_vec3().extend(m.extension.sun.w);
        }
    }
}

/// A face of the baked cubemap, pixels a side. It lights the world rather
/// than being looked at, so it is small on purpose: the diffuse term is a
/// cosine average over a hemisphere and the specular one is filtered down
/// from this by Bevy, and neither can see a texel.
const ENV: u32 = 64;

/// The sky baked into a cubemap, so it LIGHTS the world rather than only
/// being the backdrop to it. It is the same `atmos::sky` the dome runs,
/// marched once a texel on the CPU, which is why a shadowed face comes out
/// the colour of the sky above it and not pitch black.
///
/// The faces are wgpu's cubemap order (+X, -X, +Y, -Y, +Z, -Z) stacked
/// into one image of six layers, which is what a cube view is made from.
pub fn bake_env(air: &Air, sun: DVec3, eye: DVec3) -> Image {
    let n = ENV as i32;
    let mut data: Vec<u8> = Vec::with_capacity((ENV * ENV * 6 * 8) as usize);
    for face in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let u = (x as f64 + 0.5) / n as f64 * 2.0 - 1.0;
                let v = (y as f64 + 0.5) / n as f64 * 2.0 - 1.0;
                let dir = cube_dir(face, u, v);
                let (colour, _) = atmos::sky(air, eye, dir, sun);
                let lit = colour * atmos::NITS;
                for c in [lit.x, lit.y, lit.z, 1.0] {
                    data.extend_from_slice(&half::f16::from_f64(c).to_le_bytes());
                }
            }
        }
    }
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: ENV,
            height: ENV,
            depth_or_array_layers: 6,
        },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba16Float,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// Which way a texel of a cubemap face looks, in wgpu's own face order
/// and with its own handedness: the one table this file needs and the one
/// thing a bake gets wrong silently, since a sky is much the same colour
/// whichever way round it is put.
fn cube_dir(face: usize, u: f64, v: f64) -> DVec3 {
    match face {
        0 => DVec3::new(1.0, -v, -u),
        1 => DVec3::new(-1.0, -v, u),
        2 => DVec3::new(u, 1.0, v),
        3 => DVec3::new(u, -1.0, -v),
        4 => DVec3::new(u, -v, 1.0),
        _ => DVec3::new(-u, -v, -1.0),
    }
    .normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use freeport_core::day;

    /// THE SUN GOES OUT ON THE NIGHT SIDE, which is tenebris's own rule
    /// (`hex.vs.glsl`: the terminator is measured on the RADIAL and it
    /// scales the sun's diffuse) and what stops a wall at midnight being
    /// lit from under the ground. Driven through the real system,
    /// because the rule is the core's and the wiring is where an app
    /// side bug would be.
    #[test]
    fn a_wall_at_midnight_takes_no_sun_through_the_ground() {
        let here = DVec3::new(0.2, 0.3, 0.93).normalize();
        let noon = DVec3::new(0.9, 0.1, 0.42).normalize();
        // Six and eighteen are DUSK and not night: the sun is crossing
        // the horizon there and `day::daylight` is a band eight degrees
        // either side of it, which is the point of a band.
        let mut dusk = 0.0f32;
        for (hour, want) in [
            (12.0, Some(SUN_LUX)),
            (0.0, Some(0.0)),
            (22.0, Some(0.0)),
            (2.0, Some(0.0)),
            (18.0, None),
        ] {
            let start = day::at_oclock(noon, here, hour, day::DAY);
            let mut app = App::new();
            app.insert_resource(Time::<()>::default())
                .insert_resource(crate::Eye(freeport_core::pos::WorldPos(here * 1_000_000.0)))
                .insert_resource(crate::Ground(
                    std::sync::Arc::new(crate::world::World {
                        planet: freeport_core::field::Planet::default(),
                        towns: Vec::new(),
                        roads: Vec::new(),
                        routes: Vec::new(),
                        bounds: freeport_core::walker::Bounds {
                            radius: 1_000_000.0,
                            floor: 0.0,
                            top: 1.0,
                            sea: 0.0,
                        },
                        sea: freeport_core::water::Sea {
                            radius: 1_000_000.0,
                        },
                    }),
                    DVec3::ZERO,
                ))
                .insert_resource(Weather {
                    air: freeport_core::atmos::Air::round(1_000_000.0, 8_000.0),
                    sea: 1_000_000.0,
                    sun: day::sun_at(noon, start, day::DAY),
                    noon,
                    start,
                    now: start,
                    day: day::DAY,
                    here,
                })
                .add_systems(Update, turn_sun);
            let light = app
                .world_mut()
                .spawn((DirectionalLight::default(), Transform::default()))
                .id();
            app.update();
            let lux = app
                .world()
                .get::<DirectionalLight>(light)
                .expect("the sun")
                .illuminance;
            match want {
                Some(want) => assert!(
                    (lux - want).abs() < SUN_LUX * 1e-3,
                    "at {hour} o'clock the sun is worth {lux} lux and should be {want}"
                ),
                None => dusk = lux,
            }
        }
        assert!(
            dusk > 0.0 && dusk < SUN_LUX,
            "dusk is worth {dusk} lux, which is not a band"
        );
    }
}
