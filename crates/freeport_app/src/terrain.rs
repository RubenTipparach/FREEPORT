//! The material the ground wears: the Material Maker sets on three planes.
//!
//! Three array textures, a layer a set, bound once whatever the count of
//! sets, which is the mockup's answer to a real GPU's sixteen samplers.
//! The sets are decoded straight off the checkout at startup (there is no
//! asset pipeline yet, and the maps are the committed bakes), and the
//! fragment shader (`terrain.wgsl`) transcribes the mockup's: rock on the
//! steep, grass on the flat, concrete wherever the mesher says a triangle
//! is built, on Bevy's own PBR lighting. A chunk's mesh carries the
//! material in its vertex colour, flat per triangle, and its normals
//! creased where a corner disagrees with its face.

use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::DVec3;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use freeport_core::dc::DcMesh;
use freeport_core::town::Frame;
use std::path::PathBuf;

/// The sets, in the order the shader's layers name them.
pub const SETS: [&str; 5] = ["basalt", "dunes", "grass", "concrete", "hull_plate"];
/// Metres a tile, on the ground and on concrete. The ground's is what a
/// strand of the hay is long: at four metres a blade was a metre and the
/// grass read as a ploughed field at a grazing angle.
const GROUND_TILE: f32 = 2.0;
const CONCRETE_TILE: f32 = 3.0;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, Terrain>;

/// Town frames the shader may be handed: a fixed uniform array, because a
/// shader has no other kind.
pub const FRAMES: usize = 16;

/// What the shader is handed beyond the standard material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Terrain {
    /// x: metres a tile on the ground, y: on concrete, z: how many town
    /// frames are set, w: the sea's radius, which is what the sand band is
    /// measured from.
    #[uniform(100)]
    pub params: Vec4,
    /// The planet's centre in the render frame.
    #[uniform(100)]
    pub centre: Vec4,
    /// Each town's frame, three lanes a town: its direction with the
    /// radius its ground is at in w, its east, its north. Concrete, plate
    /// and street are mapped in the nearest town's frame, so a panel is
    /// level and plumb on every wall of every building, which stands on
    /// that frame's heading.
    #[uniform(100)]
    pub frames: [Vec4; FRAMES * 3],
    /// The sky at the horizon, and how much of it is in the way per metre
    /// of view distance: `sky::drift_sky` hands it down off the core's own
    /// march, so the ground fades into the sky it stands under.
    #[uniform(100)]
    pub fog: Vec4,
    /// The ground fog's shape: x how many metres its density falls off
    /// over, y how much thicker it is at the sea than the plain haze, z
    /// the radius it is measured from. Air pools in the LOW ground, so a
    /// valley is hazier than the ridge over it and a mountain stands out
    /// of its own weather.
    #[uniform(100)]
    pub haze: Vec4,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub normal: Handle<Image>,
    #[texture(105, dimension = "2d_array")]
    #[sampler(106)]
    pub orm: Handle<Image>,
}

impl MaterialExtension for Terrain {
    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/terrain.wgsl".into()
    }
}

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default());
    }
}

/// Where the assets are: `FREEPORT_ASSETS`, else the checkout this was
/// built from, else `assets` beside the working directory.
fn assets_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(root) = std::env::var("FREEPORT_ASSETS") {
        candidates.push(PathBuf::from(root));
    }
    candidates.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets"
    )));
    candidates.push(PathBuf::from("assets"));
    candidates
        .into_iter()
        .find(|c| c.join("textures/terrain").is_dir())
}

/// Where the baked sets are: under the one asset root, if the bakes are
/// there.
fn textures_dir() -> Option<PathBuf> {
    assets_dir()
        .map(|d| d.join("textures/terrain"))
        .filter(|d| d.join("basalt_albedo.png").exists())
}

/// One kind of map for every set, stacked into an array texture that
/// repeats and filters linearly. Missing or mismatched maps make a flat
/// grey layer, so the harness runs off a checkout without the bakes.
fn stack(dir: Option<&PathBuf>, kind: &str, srgb: bool) -> Image {
    let mut layers: Vec<Vec<u8>> = Vec::new();
    let mut size = (1u32, 1u32);
    for set in SETS {
        let decoded = dir
            .and_then(|d| image::open(d.join(format!("{set}_{kind}.png"))).ok())
            .map(|i| i.to_rgba8());
        match decoded {
            Some(img) if layers.is_empty() || (img.width(), img.height()) == size => {
                size = (img.width(), img.height());
                layers.push(img.into_raw());
            }
            _ => {
                warn!("no {set}_{kind}.png of the sets' size: a flat layer instead");
                let flat = if kind == "normal" {
                    [128, 128, 255, 255]
                } else {
                    [140, 140, 140, 255]
                };
                layers.push(flat.repeat((size.0 * size.1) as usize));
            }
        }
    }
    // Every layer carries its whole mip chain, layer major, which is the
    // order wgpu reads by default: without the chain a 4 m tile of grass
    // seen from 70 m up is one texel a pixel picked at random, which is
    // the noise the far ground read as.
    let mut data = Vec::new();
    let mut levels = 1;
    for layer in &layers {
        let (chain, count) = mips(layer, size.0, size.1);
        data.extend_from_slice(&chain);
        levels = count;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: SETS.len() as u32,
        },
        TextureDimension::D2,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY,
        ..default()
    });
    image
}

/// Samples a grazing ground gets across a pixel: the streets and the
/// grass are looked along, never down at.
const ANISOTROPY: u16 = 8;

/// A layer's mip chain, the level itself first and each level after it
/// the box filter of the one before, down to one texel; and how many.
fn mips(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let mut chain = rgba.to_vec();
    let mut level = rgba.to_vec();
    let (mut w, mut h) = (width as usize, height as usize);
    let mut count = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let (sx, sy) = (w / nw, h / nh);
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for dy in 0..sy {
                        for dx in 0..sx {
                            sum += level[((y * sy + dy) * w + x * sx + dx) * 4 + c] as u32;
                        }
                    }
                    next[(y * nw + x) * 4 + c] = (sum / (sx * sy) as u32) as u8;
                }
            }
        }
        chain.extend_from_slice(&next);
        level = next;
        (w, h) = (nw, nh);
        count += 1;
    }
    (chain, count)
}

/// The town frames as the shader takes them: three lanes a town, the
/// count in the return's second half, and a warning for any past the
/// array, which are mapped in their nearest neighbour's frame.
pub(crate) fn frame_lanes(frames: &[Frame]) -> ([Vec4; FRAMES * 3], f32) {
    let mut lanes = [Vec4::ZERO; FRAMES * 3];
    if frames.len() > FRAMES {
        warn!(
            "{} town frames and room for {}: the rest are mapped in a neighbour's",
            frames.len(),
            FRAMES
        );
    }
    for (i, f) in frames.iter().take(FRAMES).enumerate() {
        lanes[i * 3] = f.dir.as_vec3().extend(f.base as f32);
        lanes[i * 3 + 1] = f.east.as_vec3().extend(0.0);
        lanes[i * 3 + 2] = f.north.as_vec3().extend(0.0);
    }
    (lanes, frames.len().min(FRAMES) as f32)
}

/// The ground's material, with the sets loaded and the towns' frames set.
/// The three array textures the sets are stacked into, in the order the
/// shader binds them. The ground and everything built on it wear
/// the same three, which is why this is a function and not a line inside
/// one material's constructor.
pub fn terrain_maps(images: &mut Assets<Image>) -> [Handle<Image>; 3] {
    let dir = textures_dir();
    [
        images.add(stack(dir.as_ref(), "albedo", true)),
        images.add(stack(dir.as_ref(), "normal", false)),
        images.add(stack(dir.as_ref(), "orm", false)),
    ]
}

pub fn terrain_material(
    images: &mut Assets<Image>,
    materials: &mut Assets<TerrainMaterial>,
    frames: &[Frame],
    sea: f32,
) -> Handle<TerrainMaterial> {
    let dir = textures_dir();
    match &dir {
        Some(d) => info!("terrain sets from {}", d.display()),
        None => warn!("no baked sets found: flat colours"),
    }
    let (lanes, count) = frame_lanes(frames);
    let [albedo, normal, orm] = terrain_maps(images);
    materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        },
        extension: Terrain {
            params: Vec4::new(GROUND_TILE, CONCRETE_TILE, count, sea),
            centre: Vec4::ZERO,
            frames: lanes,
            fog: Vec4::ZERO,
            haze: Vec4::ZERO,
            albedo,
            normal,
            orm,
        },
    })
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
///
/// Every vertex carries FOUR numbers besides its place: the triangle's
/// material in the colour's red, flat, every corner the same so no
/// driver's choice of provoking vertex can change it, and the position the
/// SHADER MAPS FROM in the other three, which `place` answers along with
/// how far over the sea the vertex stands (the `uv`).
///
/// That mapping position is the whole reason this signature has a closure
/// in it. The shader used to work it out itself, from
/// `world_position - planet_centre`, and at a thousand kilometres that
/// vector's own `f32` steps in 6.25 cm: a two metre tile of grass came out
/// sampled at 32 steps rather than continuously, and since a GPU picks its
/// mip level off the DERIVATIVE of a texture coordinate, a coordinate that
/// is a staircase has a derivative of nought along each tread and a spike
/// at every riser, so the mip choice was noise. It is the rule this
/// project already keeps for meshes, arriving at the one place that still
/// broke it: nothing forms a planet scale number in `f32` and then
/// measures centimetres inside it. The caller computes the position in
/// `f64` where it is small and exact, and the fragment does no arithmetic
/// on it at all.
pub fn to_mesh(m: &DcMesh, place: impl Fn(Vec3) -> (Vec3, f32)) -> Mesh {
    let mut positions = Vec::with_capacity(m.indices.len());
    let mut normals = Vec::with_capacity(m.indices.len());
    let mut colours = Vec::with_capacity(m.indices.len());
    let mut uvs = Vec::with_capacity(m.indices.len());
    for (t, material) in m.indices.chunks(3).zip(&m.materials) {
        let p: Vec<Vec3> = t
            .iter()
            .map(|&i| Vec3::from(m.positions[i as usize]))
            .collect();
        let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        for (k, &i) in t.iter().enumerate() {
            let n = Vec3::from(m.normals[i as usize]);
            let (map, over) = place(p[k]);
            positions.push(p[k].to_array());
            normals.push(if n.angle_between(face) > CREASE {
                face.to_array()
            } else {
                n.to_array()
            });
            colours.push([*material as f32, map.x, map.y, map.z]);
            uvs.push([over, 0.0]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

/// Where a chunk's vertex is mapped from: its planet relative position
/// reduced MODULO the ground's own tile, so the number the shader maps
/// with is metres rather than a planet's radius, and two chunks that meet
/// still agree, because a triplanar tiling is periodic and congruence
/// modulo the tile is all a seam needs. The height over the sea rides
/// along, measured in `f64` for the same reason: it is a band a metre and
/// a half wide and it was quantised at six centimetres.
pub fn chunk_mapping(corner: DVec3, sea: f64) -> impl Fn(Vec3) -> (Vec3, f32) {
    let tile = GROUND_TILE as f64;
    let anchor = DVec3::new(
        corner.x.rem_euclid(tile),
        corner.y.rem_euclid(tile),
        corner.z.rem_euclid(tile),
    );
    move |p: Vec3| {
        let local = p.as_dvec3();
        (
            (anchor + local).as_vec3(),
            ((corner + local).length() - sea) as f32,
        )
    }
}
