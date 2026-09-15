//! The sea's surface, drawn: tenebris's water shader on Bevy's own
//! transmission.
//!
//! The sheet is a material extension on the standard material with the
//! standard material's screen space transmission turned on, so what is
//! seen through it is the ground behind it, refracted and attenuated by
//! Beer's law over the depth of water the view ray actually crosses, which
//! the fragment stage reads off the depth prepass. What the extension adds
//! is tenebris's: the swell, the ripples, the fresnel sky and the foam
//! (`water.wgsl` names the GLSL it transcribes). The numbers are
//! tenebris's `water.yaml`.

use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use freeport_core::dc::DcMesh;
use freeport_core::water::SURFACE;

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExt>;

/// What the shader is handed beyond the standard material. One uniform;
/// the comments in `water.wgsl` say what each lane is.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterExt {
    #[uniform(100)]
    pub centre: Vec4,
    #[uniform(100)]
    pub wave: Vec4,
    #[uniform(100)]
    pub deep: Vec4,
    #[uniform(100)]
    pub horizon: Vec4,
    #[uniform(100)]
    pub zenith: Vec4,
    #[uniform(100)]
    pub foam: Vec4,
    #[uniform(100)]
    pub band: Vec4,
    /// The sky at the horizon and its density, as `terrain::Terrain::fog`.
    #[uniform(100)]
    pub fog: Vec4,
    /// The ground fog's shape, as `terrain::Terrain::haze`.
    #[uniform(100)]
    pub haze: Vec4,
}

impl MaterialExtension for WaterExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://freeport_app/water.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/water.wgsl".into()
    }

    /// The sheet stays out of the depth prepass, which is where the
    /// fragment reads the ground's depth from.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

pub struct WaterPlugin;

/// A handle held so `water_lib.wgsl` is LOADED and not merely registered,
/// which is what an import needs to resolve: a shader nobody has asked for
/// is a pipeline Bevy retries in silence.
#[derive(Resource)]
struct WaterLib(#[allow(dead_code)] Handle<Shader>);

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water_lib.wgsl");
        embedded_asset!(app, "water.wgsl");
        let lib = app
            .world()
            .resource::<AssetServer>()
            .load("embedded://freeport_app/water_lib.wgsl");
        app.insert_resource(WaterLib(lib));
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default());
    }
}

/// A mark on the sea's chunks, beside their `Chunk`.
#[derive(Component)]
pub struct Sheet;

/// The sheet's numbers, tenebris's `water.yaml`, in one place, so nothing
/// that draws the sea spells them itself.
pub fn sheet_ext(sea: f64) -> WaterExt {
    let nits = freeport_core::atmos::NITS as f32;
    WaterExt {
        centre: Vec4::new(0.0, 0.0, 0.0, sea as f32),
        wave: Vec4::new(0.75, 1.5, 0.65, 0.5),
        deep: Vec4::new(0.02, 0.10, 0.22, 2.0),
        // The three the shader takes through the camera's exposure are in
        // CANDELA, `atmos::NITS` times the colour they are authored as, or
        // they arrive at five ten thousandths of the ground beside them
        // and the sheet reflects nothing at all.
        horizon: (Vec3::new(0.85, 0.92, 0.98) * nits).extend(0.5),
        zenith: (Vec3::new(0.35, 0.55, 0.85) * nits).extend(1.6),
        foam: (Vec3::new(0.95, 0.97, 1.0) * nits).extend(0.10),
        band: Vec4::new(0.35, 0.60, 0.50, 1.20),
        fog: Vec4::ZERO,
        haze: Vec4::ZERO,
    }
}

/// The standard material under the sheet, likewise shared: transmissive,
/// smooth, and attenuating over the water it is looked through.
pub fn sheet_base() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.55, 0.8, 0.9),
        perceptual_roughness: 0.06,
        metallic: 0.0,
        reflectance: 0.35,
        specular_transmission: 0.92,
        ior: 1.33,
        thickness: 2.0,
        attenuation_distance: 6.0,
        attenuation_color: Color::srgb(0.02, 0.30, 0.45),
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// The sea's material, on tenebris's numbers: `sea` is its radius.
pub fn water_material(materials: &mut Assets<WaterMaterial>, sea: f64) -> Handle<WaterMaterial> {
    materials.add(WaterMaterial {
        base: sheet_base(),
        extension: sheet_ext(sea),
    })
}

/// A chunk's sea surface as a Bevy mesh: the surface triangles only,
/// vertices shared, normals the field's. None if nothing is drawn.
pub fn to_sheet(m: &DcMesh) -> Option<Mesh> {
    let mut indices: Vec<u32> = Vec::new();
    for (t, material) in m.indices.chunks(3).zip(&m.materials) {
        if *material == SURFACE {
            indices.extend_from_slice(t);
        }
    }
    if indices.is_empty() {
        return None;
    }
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone())
        .with_inserted_indices(bevy::mesh::Indices::U32(indices)),
    )
}

/// Keep the sea's centre with the origin, as the terrain's is.
pub fn recentre(
    materials: &mut Assets<WaterMaterial>,
    handle: &Handle<WaterMaterial>,
    centre: Vec3,
) {
    if let Some(m) = materials.get_mut(handle) {
        m.extension.centre = centre.extend(m.extension.centre.w);
    }
}
