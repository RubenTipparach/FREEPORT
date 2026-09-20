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

/// The terrain material the ground and everything built on it wear, and
/// the one a ROAD wears, kept as a resource so a town raised mid flight
/// can reach them.
///
/// TWO handles and one shader: the tarmac's is the same material with a
/// DEPTH BIAS on it, because a road drawn in the corridor the mesher cut
/// for it is a surface a hair over another surface, and a hair is what a
/// depth buffer argues about. It is SMALL on purpose. A constant bias
/// buys clearance that grows as the square of the distance under an
/// infinite reverse Z projection, so a value that is nothing underfoot
/// is centimetres a few hundred metres out, which is all the near road
/// needs: past that the ground PAINTS the road itself
/// (`field::PAINT_FROM`). A bias big enough to beat a HILL would show
/// the road through it, and a hill occluding a road is what a hill is
/// for.
#[derive(bevy::prelude::Resource)]
pub struct Ground3d {
    pub ground: Handle<TerrainMaterial>,
    pub tarmac: Handle<TerrainMaterial>,
}

use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use freeport_core::biome::{self, Climate};
use freeport_core::dc::DcMesh;
use freeport_core::town::Frame;
use std::path::PathBuf;

/// The sets, in the order the shader's layers name them.
///
/// The last five are what a BUILDING is made of, and they are the
/// owner's own list of trades: a house out of wood, red brick or vinyl
/// and an office out of red brick, concrete, marble, glass or stone
/// blocks. They cost one array layer each and NOT a draw call, a shader
/// or a material, because `terrain.wgsl` picks its layer off the
/// triangle's own material byte and skips every set whose weight is
/// nought (`tri` returns early and `textureSampleGrad` carries the
/// derivative past the branch), so a wall of brick pays for brick and
/// nothing else on the list.
pub const SETS: [&str; 12] = [
    "basalt",
    "dunes",
    "grass",
    "concrete",
    "hull_plate",
    "asphalt",
    "wood",
    "brick",
    "vinyl",
    "marble",
    "stone",
    "glazing",
];
/// Metres a tile, on the ground and on concrete. The ground's is what a
/// strand of the hay is long: at four metres a blade was a metre and the
/// grass read as a ploughed field at a grazing angle.
const GROUND_TILE: f32 = 2.0;
const CONCRETE_TILE: f32 = 3.0;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, Terrain>;

/// Town frames the shader may be handed: a fixed uniform array, because a
/// shader has no other kind.
pub const FRAMES: usize = 16;

/// How hard a town's lamps and its lit windows burn at full night, as a
/// share of what they were drawn at before there was a night: one. By
/// day they go to nought, because a pane glowing at noon reads as a
/// shading defect and a street lamp lit at noon reads as a waste.
const LAMPS_AT_NIGHT: f32 = 1.0;

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
    /// Body surface colour; w blends from the authored texture colours.
    #[uniform(100)]
    pub palette: Vec4,
    /// Where the sun is in the world frame, xyz, and how hard a lamp
    /// burns at full night in w. `sky::drift_sky` hands it down, and the
    /// fragment measures the terminator on the body's own RADIAL by
    /// transcribing `freeport_core::day::daylight`, which is what
    /// `distant.wgsl` and `water.wgsl` already do: one rule in the core
    /// and three transcriptions, so the night falls in one place on the
    /// ground, on the sea and on the body seen from orbit.
    #[uniform(100)]
    pub sun: Vec4,
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
pub(crate) fn assets_dir() -> Option<PathBuf> {
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

/// How hard the tarmac is pushed toward the camera in the depth buffer.
///
/// Small, because the near road only has to beat the two millimetres a
/// dual contoured plane is held to and the centimetre a corridor's own
/// mitre stands proud at a bend, and because a bias that beat a HILL
/// would draw the road through it.
const TARMAC_BIAS: f32 = 8.0;

pub fn terrain_material(
    images: &mut Assets<Image>,
    materials: &mut Assets<TerrainMaterial>,
    frames: &[Frame],
    sea: f32,
) -> Ground3d {
    let dir = textures_dir();
    match &dir {
        Some(d) => info!("terrain sets from {}", d.display()),
        None => warn!("no baked sets found: flat colours"),
    }
    let (lanes, count) = frame_lanes(frames);
    let [albedo, normal, orm] = terrain_maps(images);
    let of = |bias: f32| ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            depth_bias: bias,
            ..default()
        },
        extension: Terrain {
            params: Vec4::new(GROUND_TILE, CONCRETE_TILE, count, sea),
            centre: Vec4::ZERO,
            frames: lanes,
            fog: Vec4::ZERO,
            haze: Vec4::ZERO,
            palette: Vec4::ZERO,
            sun: Vec4::new(0.0, 1.0, 0.0, LAMPS_AT_NIGHT),
            albedo: albedo.clone(),
            normal: normal.clone(),
            orm: orm.clone(),
        },
    };
    Ground3d {
        ground: materials.add(of(0.0)),
        tarmac: materials.add(of(TARMAC_BIAS)),
    }
}

/// A corner whose normal is further than this from its triangle's face is
/// shaded on the face's normal, radians: a box's edge, never a slope of
/// ground.
const CREASE: f32 = 0.35;

/// The chunk as Bevy draws it. Vertices are split per triangle, and each
/// terrain corner keeps the field's smooth normal, including at LOD joins.
/// Substituting a face normal there gives a shared vertex two normals and
/// makes a shading seam. Architectural materials retain the crease rule:
/// a corner far from its face normal takes the face's normal instead.
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
pub fn to_mesh(m: &DcMesh, place: impl Fn(Vec3) -> Vertex) -> Mesh {
    to_mesh_filtered(m, place, |_| true)
}

/// Separate glass from opaque structure without duplicating the model or
/// changing its mapping frame. Both meshes retain the baked normals.
pub fn to_mesh_filtered(
    m: &DcMesh,
    place: impl Fn(Vec3) -> Vertex,
    include: impl Fn(u8) -> bool,
) -> Mesh {
    let mut positions = Vec::with_capacity(m.indices.len());
    let mut normals = Vec::with_capacity(m.indices.len());
    let mut colours = Vec::with_capacity(m.indices.len());
    let mut uvs = Vec::with_capacity(m.indices.len());
    let mut climates = Vec::with_capacity(m.indices.len());
    for (t, material) in m.indices.chunks(3).zip(&m.materials) {
        if !include(*material) {
            continue;
        }
        let p: [Vec3; 3] = std::array::from_fn(|k| Vec3::from(m.positions[t[k] as usize]));
        let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        for (k, &i) in t.iter().enumerate() {
            let n = Vec3::from(m.normals[i as usize]);
            let v = place(p[k]);
            positions.push(p[k].to_array());
            normals.push(
                if *material != freeport_core::field::TERRAIN && n.angle_between(face) > CREASE {
                    face.to_array()
                } else {
                    n.to_array()
                },
            );
            colours.push([*material as f32, v.map.x, v.map.y, v.map.z]);
            uvs.push([v.over_sea, v.temp]);
            climates.push([v.wet, 0.0]);
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
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, climates)
}

/// Where a chunk's vertex is mapped from: its planet relative position
/// reduced MODULO the ground's own tile, so the number the shader maps
/// with is metres rather than a planet's radius, and two chunks that meet
/// still agree, because a triplanar tiling is periodic and congruence
/// modulo the tile is all a seam needs. The height over the sea rides
/// along, measured in `f64` for the same reason: it is a band a metre and
/// a half wide and it was quantised at six centimetres.
pub fn chunk_mapping(
    corner: DVec3,
    sea: f64,
    shape: Option<biome::Shape>,
) -> impl Fn(Vec3) -> Vertex {
    let map = ground_mapping(corner, sea, shape);
    move |p: Vec3| map(corner + p.as_dvec3())
}

/// The same, from a planet relative WORLD position rather than from a
/// chunk's own local one: what a road's MOUND is mapped with, since its
/// triangles are in a stretch's frame and not in a chunk's.
///
/// `anchor` is any point near them, and it is what is reduced modulo the
/// tile: the REDUCTION has to be constant over a mesh and the offset
/// from it continuous, or a triangle straddling a tile boundary would
/// have its coordinate wrap inside it, and a GPU picks its mip level off
/// that coordinate's derivative. A stretch is 5.5 km across, so an `f32`
/// of the offset holds a third of a millimetre.
pub fn ground_mapping(
    anchor: DVec3,
    sea: f64,
    shape: Option<biome::Shape>,
) -> impl Fn(DVec3) -> Vertex {
    let tile = GROUND_TILE as f64;
    let base = DVec3::new(
        anchor.x.rem_euclid(tile),
        anchor.y.rem_euclid(tile),
        anchor.z.rem_euclid(tile),
    );
    move |at: DVec3| {
        let over = at.length() - sea;
        // The climate at this vertex, so the ground a walker stands on is
        // the same biome the body's chart paints from orbit. It is asked
        // PER VERTEX rather than per chunk because a chunk wide tint would
        // put a hard line down every chunk boundary on the planet, and the
        // thing a biome has to do is blend.
        let weather = shape.map_or(
            Climate {
                temp: 1.0,
                wet: 0.5,
            },
            |s| s.climate(at.normalize_or(DVec3::Y), over),
        );
        Vertex {
            map: (base + (at - anchor)).as_vec3(),
            over_sea: over as f32,
            temp: weather.temp as f32,
            wet: weather.wet as f32,
        }
    }
}

/// What a vertex carries besides where it is: the position the shader maps
/// FROM, how far over the sea it stands, and the climate there.
#[derive(Clone, Copy)]
pub struct Vertex {
    pub map: Vec3,
    pub over_sea: f32,
    pub temp: f32,
    pub wet: f32,
}

impl Vertex {
    /// A built thing's, which has no climate of its own: concrete is
    /// concrete in a desert and on a glacier.
    pub fn built(map: Vec3, over_sea: f32) -> Vertex {
        Vertex {
            map,
            over_sea,
            temp: 1.0,
            wet: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use freeport_core::field::{CONCRETE, TERRAIN};

    #[test]
    fn terrain_keeps_shared_normals_when_lod_faces_have_different_slopes() {
        let mut mesh = DcMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0],
                [0.0, 1.0, 1.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            levels: vec![0, 1, 0, 1],
            indices: vec![0, 1, 2, 0, 3, 1],
            materials: vec![TERRAIN; 2],
            ..default()
        };
        let rendered = to_mesh(&mesh, |p| Vertex::built(p, 0.0));
        let Some(VertexAttributeValues::Float32x3(normals)) =
            rendered.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("missing normals")
        };
        assert!(normals.iter().all(|n| *n == [0.0, 1.0, 0.0]));
        // Architectural creases still retain their hard edges.
        mesh.materials = vec![CONCRETE; 2];
        let rendered = to_mesh(&mesh, |p| Vertex::built(p, 0.0));
        let Some(VertexAttributeValues::Float32x3(normals)) =
            rendered.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("missing normals")
        };
        assert_ne!(normals[0], normals[3]);
    }
}
