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
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use freeport_core::dc::DcMesh;
use std::path::PathBuf;

/// The sets, in the order the shader's layers name them.
pub const SETS: [&str; 3] = ["basalt", "grass", "concrete"];
/// Metres a tile, on the ground and on concrete.
const GROUND_TILE: f32 = 4.0;
const CONCRETE_TILE: f32 = 3.0;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, Terrain>;

/// What the shader is handed beyond the standard material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Terrain {
    /// x: metres a tile on the ground, y: on concrete.
    #[uniform(100)]
    pub params: Vec4,
    /// The planet's centre in the render frame.
    #[uniform(100)]
    pub centre: Vec4,
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

/// Where the baked sets are: `FREEPORT_ASSETS`, else the checkout this was
/// built from, else `assets` beside the working directory.
fn textures_dir() -> Option<PathBuf> {
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
        .map(|c| c.join("textures/terrain"))
        .find(|d| d.join("basalt_albedo.png").exists())
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
    let data = layers.concat();
    let mut image = Image::new(
        Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: SETS.len() as u32,
        },
        TextureDimension::D2,
        data,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// The ground's material, with the sets loaded.
pub fn terrain_material(
    images: &mut Assets<Image>,
    materials: &mut Assets<TerrainMaterial>,
) -> Handle<TerrainMaterial> {
    let dir = textures_dir();
    match &dir {
        Some(d) => info!("terrain sets from {}", d.display()),
        None => warn!("no baked sets found: flat colours"),
    }
    let albedo = images.add(stack(dir.as_ref(), "albedo", true));
    let normal = images.add(stack(dir.as_ref(), "normal", false));
    let orm = images.add(stack(dir.as_ref(), "orm", false));
    materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        },
        extension: Terrain {
            params: Vec4::new(GROUND_TILE, CONCRETE_TILE, 0.0, 0.0),
            centre: Vec4::ZERO,
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
/// side of it is continuous. The vertex colour carries the triangle's
/// material in red and the vertex's level in green, flat, every corner the
/// same, so no driver's choice of provoking vertex can change either.
pub fn to_mesh(m: &DcMesh) -> Mesh {
    let mut positions = Vec::with_capacity(m.indices.len());
    let mut normals = Vec::with_capacity(m.indices.len());
    let mut colours = Vec::with_capacity(m.indices.len());
    for (t, material) in m.indices.chunks(3).zip(&m.materials) {
        let p: Vec<Vec3> = t
            .iter()
            .map(|&i| Vec3::from(m.positions[i as usize]))
            .collect();
        let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        for (k, &i) in t.iter().enumerate() {
            let n = Vec3::from(m.normals[i as usize]);
            positions.push(p[k].to_array());
            normals.push(if n.angle_between(face) > CREASE {
                face.to_array()
            } else {
                n.to_array()
            });
            colours.push([*material as f32, m.levels[i as usize] as f32, 0.0, 1.0]);
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
