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

use crate::terrain::TerrainMaterial;
use crate::tiers::{SeaMaterial, TierMaterial};
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

/// How far out the dome sits, metres. It rides the eye and is drawn
/// behind everything, so all it has to be is further than anything else
/// in the scene: a planet's far side from orbit is tens of kilometres and
/// this is half a thousand.
const DOME: f32 = 500_000.0;

/// What the dome's shader is handed: the planet, the sun and the air.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Sky {
    /// The planet's centre in the render frame, w: the dome's radius.
    #[uniform(100)]
    pub centre: Vec4,
    /// Where the sun is, as a direction.
    #[uniform(100)]
    pub sun: Vec4,
    /// x: the ground's radius, y: the shell's.
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
            shell: Vec4::new(air.ground as f32, air.top as f32, 0.0, 0.0),
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
    /// Where the sun is, as a direction in the world frame.
    pub sun: DVec3,
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
        app.add_plugins(MaterialPlugin::<Sky>::default());
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
    info!(
        "sky: air from {:.0} m to {:.0} m, the sun at {:.2}, fog {:.4} a metre fading out by {:.0} m",
        weather.air.ground, weather.air.top, weather.sun, weather.air.fog, weather.air.fog_height
    );
}

/// The materials the fog is painted onto: the dual contoured ground and
/// sea, and the hex world's two tiers and its sheet. One system hands all
/// of them the same colour and the same density, because they all stand
/// under one sky.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Painted<'w> {
    pub ground: ResMut<'w, Assets<TerrainMaterial>>,
    pub tiers: ResMut<'w, Assets<TierMaterial>>,
    pub water: ResMut<'w, Assets<WaterMaterial>>,
    pub seas: ResMut<'w, Assets<SeaMaterial>>,
}

/// Every frame: the dome onto the eye, the planet's centre and the sun
/// into it, and this air's own horizon into everything that fades with
/// distance.
pub fn drift_sky(
    eye: Res<crate::Eye>,
    frame: Res<crate::stream::Frame>,
    weather: Res<Weather>,
    mut skies: ResMut<Assets<Sky>>,
    mut dome: Query<&mut Transform, With<Dome>>,
    mut painted: Painted,
) {
    let here = eye.0 .0;
    let centre = frame.0.local(freeport_core::pos::WorldPos(DVec3::ZERO));
    let at = frame.0.local(eye.0);
    for mut tf in &mut dome {
        tf.translation = at;
    }
    let ids: Vec<_> = skies.ids().collect();
    for id in ids {
        if let Some(sky) = skies.get_mut(id) {
            sky.centre = centre.extend(DOME);
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
        }
    }
    let ids: Vec<_> = painted.tiers.ids().collect();
    for id in ids {
        if let Some(m) = painted.tiers.get_mut(id) {
            m.extension.fog = fog;
            m.extension.haze = haze;
        }
    }
    let ids: Vec<_> = painted.water.ids().collect();
    for id in ids {
        if let Some(m) = painted.water.get_mut(id) {
            m.extension.fog = fog;
            m.extension.haze = haze;
        }
    }
    let ids: Vec<_> = painted.seas.ids().collect();
    for id in ids {
        if let Some(m) = painted.seas.get_mut(id) {
            m.extension.fog = fog;
            m.extension.haze = haze;
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
