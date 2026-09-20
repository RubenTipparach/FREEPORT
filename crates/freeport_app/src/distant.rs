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
    /// xyz the way the SUN lies, as a unit direction in the world, and w
    /// how bright a city burns on the night side. A direction needs no
    /// rebasing, because the render frame is a translation of the world's
    /// and nothing else.
    #[uniform(100)]
    pub sun: Vec4,
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
/// kilometres, which is about the coarsest ring of chunks (4,096 m cells
/// at fourteen levels) and so about where the streamed ground takes over
/// anyway. Wider costs the bake: it is one `Planet::surface` a texel and
/// the surface is the whole biome model, so four times the texels is four
/// times the seconds.
///
/// The comment said 2,048 while the constant said 1,024, which is a
/// doubling of blur nothing in the file admitted to: at 1,024 a texel is
/// 6,136 m, and the picture from 400 km up handed a 460 m pixel a texel
/// thirteen pixels wide. It also halves what the CITY and ROAD marks lie
/// by, since both are measured in texels: a city goes from 13.5 km to
/// 6.7 and a road from 5.5 to 2.8.
pub const CHART_W: usize = 2048;
pub const CHART_H: usize = CHART_W / 2;

/// How bright a body's cities burn on its night side, in the linear
/// units the emissive is written in.
///
/// Well over one on purpose: it is what a city is worth against a
/// hemisphere lit by nothing but the sky's own floor, and it is what
/// carries a town through the bloom threshold at the two or three pixels
/// one is drawn at from orbit. The chart's light mask is nought to one
/// and this is what it scales.
const LAMPS: f32 = 120.0;

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
/// drawn, and a chunk at the coarsest level is a kilometres wide cell
/// whose surface stands its own error off the true one. A share of the
/// relief is 80 m on the harness planet, which is under a thousandth of
/// its radius and invisible from orbit, against the four kilometres the
/// old sphere sat at (the bottom of the whole band) which is what made
/// the join show.
///
/// Eighty metres is only enough because the displacement is a LOWER
/// ENVELOPE now: see `floor` below. Sampled at its own vertices the
/// sphere stood KILOMETRES over the true ground between them, and the
/// chart poked up through the coarse terrain all over a picture from a
/// hundred and fifty kilometres up.
const SINK: f64 = 0.01;

/// How far round a vertex the LOWER ENVELOPE looks, as a share of the
/// icosphere's own edge, and how many bearings it looks along.
///
/// An icosahedron's edge is 1.0515 radii, so at `MOST` subdivisions this
/// sphere's vertices stand 13 km apart on the harness planet, and what a
/// triangle strung between three of them does to a relief whose HILLS
/// have a wavelength of 13 km is alias it: the chord cuts the tops off
/// and stands over every hollow. Measured as a curvature, an interpolated
/// chord over 13 km of that term is up to 3.3 km above the ground it
/// spans, which is forty times the sink and is why the chart showed
/// through the terrain rather than under it.
///
/// Taking the LOWEST ground a vertex can see instead makes the sphere an
/// envelope UNDER the body rather than a surface through it, so it cannot
/// poke through whatever the terrain does between two of its vertices.
/// What it costs is the SILHOUETTE, which sits at the local low ground
/// rather than at the ridge: on this body that is four kilometres of a
/// thousand, which is 0.4% of the limb and nothing an eye can find.
const ENVELOPE: f64 = 1.0515;
const BEARINGS: usize = 8;

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

/// The LOWEST ground within `reach` radians of a direction, as a radius.
///
/// A ring of bearings and the point itself, which is enough because what
/// this is under is a TRIANGLE strung between three vertices this far
/// apart: the deepest the chord can be wrong by is the lowest ground it
/// spans, and a ring at the spacing finds that where a single sample at
/// the vertex cannot see it at all.
fn floor(planet: &Planet, dir: DVec3, reach: f64) -> f64 {
    let (east, north) = freeport_core::town::frame_at(dir);
    let (s, c) = reach.sin_cos();
    (0..BEARINGS)
        .map(|k| {
            let a = k as f64 / BEARINGS as f64 * std::f64::consts::TAU;
            (dir * c + (east * a.cos() + north * a.sin()) * s).normalize_or(dir)
        })
        .chain(std::iter::once(dir))
        .map(|d| planet.radius + planet.surface(d).0)
        .fold(f64::INFINITY, f64::min)
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
    let reach = ENVELOPE / subdivisions.clamp(1, MOST) as f64;
    // BARE, which is the chart's own rule (`Chart::bake_bare`) and for
    // the same two reasons: a town is 185 m across against this sphere's
    // 13 km vertices, so its levelling cannot move one; and asking a
    // planet carrying 190,000 corridor sites about it would walk them at
    // every one of the 560,000 samples this envelope takes.
    let planet = &planet.bare();
    let mut positions = Vec::with_capacity(unit.len());
    let mut normals = Vec::with_capacity(unit.len());
    let mut uvs = Vec::with_capacity(unit.len());
    for p in unit {
        let dir = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64).normalize_or(DVec3::Y);
        // The sea is a floor: a sphere that followed the ocean floor down
        // would draw the sea bed rather than the sea, and the water is a
        // surface at one radius.
        let ground = floor(planet, dir, reach);
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
            // Filled by `sky::drift_sky` every frame. Nought until then,
            // which reads as a sun along no axis at all and so as a body
            // wholly in its own day: a planet with no lights on rather
            // than a planet lit from the wrong side.
            sun: Vec4::new(0.0, 0.0, 0.0, LAMPS),
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
        // Written RGB, with the alpha DROPPED. The albedo's alpha is the
        // water mask, so a four channel dump opens with every scrap of
        // land transparent and every ocean opaque: the one picture that
        // exists so a person can look at the chart showed the planet
        // inside out in every viewer.
        let rgb: Vec<u8> = data
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let Some(buffer) = image::RgbImage::from_raw(w, h, rgb) else {
            continue;
        };
        match buffer.save(&path) {
            Ok(()) => info!("chart {what} written to {}", path.display()),
            Err(e) => warn!("chart {what} not written: {e}"),
        }
    }
}
