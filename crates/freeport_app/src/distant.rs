//! A whole body from far off: the chart uploaded, and the sphere it is
//! drawn on, displaced by the body's own relief and picked by distance.
//!
//! What this replaces stood at the BOTTOM of the relief band, four
//! kilometres under the lowest ground, smooth, and coloured one flat value
//! a vertex. From thirty kilometres up the picture was a green smear on a
//! blue ball with the streamed chunks floating somewhere over it, and the
//! join between the two was the thing the owner could see. Here the sphere
//! carries the real relief, is sunk only far enough under the chunks that
//! they always win the depth test, and is painted from `core::chart`.

use crate::terrain::assets_dir;
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use freeport_core::chart::Chart;
use freeport_core::field::Planet;

pub type DistantMaterial = ExtendedMaterial<StandardMaterial, Distant>;

/// What the distant shader is handed beyond the standard material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Distant {
    /// xyz the body's centre in the render frame, w how hard the chart's
    /// slope bends the normal.
    #[uniform(100)]
    pub centre: Vec4,
    /// The sky at the horizon and the haze per metre, as the ground's.
    #[uniform(100)]
    pub fog: Vec4,
    /// x the water's reflectance, y spare.
    #[uniform(100)]
    pub sea: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub chart: Handle<Image>,
    #[texture(103)]
    #[sampler(104)]
    pub slopes: Handle<Image>,
}

impl MaterialExtension for Distant {
    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/distant.wgsl".into()
    }
}

pub struct DistantPlugin;

impl Plugin for DistantPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "distant.wgsl");
        app.add_plugins(MaterialPlugin::<DistantMaterial>::default());
        // Only once there is a world: the frame the distance is measured
        // in is spawned with it, and a system that asks for a resource
        // before anything makes one fails its own parameter validation
        // every frame until it exists.
        app.add_systems(
            PostUpdate,
            pick_lod.run_if(resource_exists::<crate::stream::Frame>),
        );
    }
}

/// How wide the chart is, in texels. The height is half, which is what an
/// equirectangular projection of a sphere is.
///
/// A thousand kilometre planet at 2,048 wide is a texel every three
/// kilometres, which is finer than the coarsest ring of chunks and about
/// where the streamed ground takes over anyway. Wider costs the bake: it
/// is one `Planet::surface` a texel, and the surface is the whole biome
/// model.
pub const CHART_W: usize = 1024;
pub const CHART_H: usize = CHART_W / 2;

/// How far the chart's own slope bends the distant normal. It is what
/// makes a range read on a sphere no mesh could hold one on, and at much
/// over a half the terminator crawls with shading that is not there.
const BEND: f32 = 0.45;

/// The subdivisions of each level of the distant sphere, coarsest first,
/// and how many body radii away each is used up to.
///
/// A sphere is not streamed and cannot be: it is the thing that draws when
/// nothing else does. So it is a few whole meshes picked by distance, and
/// the numbers are what a body is worth on screen: from four radii up a
/// planet is a disk a few hundred pixels across and 32 subdivisions is a
/// triangle every few of them; standing off one radius it fills the view
/// and wants 128. The chart's normal map carries everything finer, which
/// is the whole reason the mesh does not have to.
const LEVELS: [(u32, f64); 3] = [(24, f64::INFINITY), (48, 6.0), (MOST, 2.5)];

/// The most subdivisions Bevy's own icosphere will build. Measured by
/// asking for more: 144 comes back `TooManyVertices` with 210,252 points
/// in the error. At 79 a body is 128,000 triangles, which is a triangle
/// every eight kilometres on the harness planet, and the chart's slope map
/// is what carries everything finer than that. Going past it means writing
/// an icosphere here rather than using Bevy's, which buys a finer
/// SILHOUETTE and nothing else, since the shading is the map's.
const MOST: u32 = 79;

/// How far under the true surface the sphere is sunk, as a share of the
/// body's relief.
///
/// It has to lose the depth test to the streamed chunks wherever they are
/// drawn, and a chunk at the coarsest level is a 128 m cell whose surface
/// stands its own error off the true one. A share of the relief is 80 m on
/// the harness planet, which is under a thousandth of its radius and
/// invisible from orbit, against the four kilometres the old sphere sat at
/// (the bottom of the whole band) which is what made the join show.
const SINK: f64 = 0.01;

/// The meshes a body can draw itself with, coarsest first.
#[derive(Component)]
pub struct DistantLod {
    pub meshes: Vec<Handle<Mesh>>,
    /// The body's centre and radius, for the distance the level is picked
    /// off. It is the ABSOLUTE position, never the rebased one, which is
    /// this repository's own rule about LOD.
    pub centre: DVec3,
    pub radius: f64,
}

/// One body's sphere at one subdivision, displaced by its own relief.
///
/// The normal stays RADIAL rather than being rebuilt off the displaced
/// triangles: the chart's slope map is what shades this surface, and it is
/// written in a tangent frame built on the radial, so a mesh normal that
/// followed the displacement would apply the relief's shading twice.
pub fn sphere(planet: &Planet, sea: f64, subdivisions: u32) -> Mesh {
    let base = Sphere::new(1.0)
        .mesh()
        .ico(subdivisions.min(MOST))
        .expect("a subdivision count held under the icosphere's own limit");
    let Some(VertexAttributeValues::Float32x3(unit)) = base.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return base;
    };
    let sink = planet.relief * SINK;
    let mut positions = Vec::with_capacity(unit.len());
    let mut normals = Vec::with_capacity(unit.len());
    let mut uvs = Vec::with_capacity(unit.len());
    for p in unit {
        let dir = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64).normalize_or(DVec3::Y);
        // The sea is a floor: a sphere that followed the ocean floor down
        // would draw the sea bed rather than the sea, and the water is a
        // surface at one radius.
        let ground = planet.radius + planet.surface(dir).0;
        let r = ground.max(sea) - sink;
        positions.push((dir * r).as_vec3().to_array());
        normals.push(dir.as_vec3().to_array());
        uvs.push([0.0, 0.0]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    if let Some(Indices::U32(i)) = base.indices() {
        mesh.insert_indices(Indices::U32(i.clone()));
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

/// The two chart images, ready to bind. The albedo is sRGB because it is a
/// colour and the slope map is not because it is a number.
pub fn images(chart: &Chart) -> (Image, Image) {
    let size = Extent3d {
        width: chart.width as u32,
        height: chart.height as u32,
        depth_or_array_layers: 1,
    };
    // Repeating in u and clamped in v: longitude wraps round the body and
    // latitude stops at the poles.
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    let mut albedo = Image::new(
        size,
        TextureDimension::D2,
        chart.albedo.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    albedo.sampler = sampler.clone();
    let mut slopes = Image::new(
        size,
        TextureDimension::D2,
        chart.normal.clone(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    slopes.sampler = sampler;
    (albedo, slopes)
}

/// The material for one body, with its chart bound.
pub fn material(chart: Handle<Image>, slopes: Handle<Image>) -> DistantMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 1.0,
            ..default()
        },
        extension: Distant {
            centre: Vec4::new(0.0, 0.0, 0.0, BEND),
            fog: Vec4::ZERO,
            sea: Vec4::new(0.35, 0.0, 0.0, 0.0),
            chart,
            slopes,
        },
    }
}

/// Which mesh each body draws itself with: the finest level whose reach
/// the eye is inside. Measured off the ABSOLUTE position, because mixing
/// the rebased one in reads every body as the same distance away the
/// moment the world goes heliocentric.
fn pick_lod(
    eye: Query<&GlobalTransform, With<Camera3d>>,
    frame: Res<crate::stream::Frame>,
    mut bodies: Query<(&DistantLod, &mut Mesh3d)>,
) {
    let Ok(camera) = eye.single() else {
        return;
    };
    let at = frame.0.world(camera.translation());
    for (lod, mut mesh) in &mut bodies {
        let away = (at.0 - lod.centre).length();
        let mut want = 0;
        for (i, (_, reach)) in LEVELS.iter().enumerate() {
            if away <= reach * lod.radius {
                want = want.max(i);
            }
        }
        let want = want.min(lod.meshes.len().saturating_sub(1));
        if mesh.0 != lod.meshes[want] {
            mesh.0 = lod.meshes[want].clone();
        }
    }
}

/// Every subdivision a body keeps a mesh of.
pub fn levels() -> impl Iterator<Item = u32> {
    LEVELS.iter().map(|(n, _)| *n)
}

/// Write a chart out as two PNGs beside the assets, for looking at. A
/// planet's own texture is a picture, and a picture nobody can open is a
/// buffer nobody can check.
pub fn dump(chart: &Chart, name: &str) {
    let Some(dir) = assets_dir() else {
        return;
    };
    let out = dir.join("charts");
    if std::fs::create_dir_all(&out).is_err() {
        return;
    }
    let (w, h) = (chart.width as u32, chart.height as u32);
    for (what, data) in [("albedo", &chart.albedo), ("slope", &chart.normal)] {
        let path = out.join(format!("{name}-{what}.png"));
        let Some(buffer) = image::RgbaImage::from_raw(w, h, data.clone()) else {
            continue;
        };
        match buffer.save(&path) {
            Ok(()) => info!("chart {what} written to {}", path.display()),
            Err(e) => warn!("chart {what} not written: {e}"),
        }
    }
}
