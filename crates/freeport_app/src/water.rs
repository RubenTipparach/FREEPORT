//! The sea's surface, drawn: tenebris's water shader on Bevy's own
//! transmission, on pale-blue-dot's measured numbers.
//!
//! The sheet is a material extension on the standard material with the
//! standard material's screen space transmission turned on, so what is
//! seen through it is the ground behind it, refracted and attenuated by
//! Beer's law over the depth of water the view ray actually crosses, which
//! the fragment stage reads off the depth prepass. What the extension adds
//! is tenebris's: the swell, the ripples, the fresnel sky and the foam
//! (`water.wgsl` names the GLSL it transcribes).
//!
//! The NUMBERS are pale-blue-dot's `assets/config/water.ron`, which are
//! tenebris's own re-measured under a tone mapper. Its
//! `openspec/changes/water-look/design.md` is the ablation, and the two
//! results worth carrying are that every shine knob in the shader together
//! is worth about one per cent of the sea's colour while ABSORPTION is
//! worth thirty times that, and that tenebris's near white horizon
//! (0.85, 0.92, 0.98) and its 0.02 of red in the deep colour are authored
//! for a renderer that clips its framebuffer. Under Bevy's default
//! `TonyMcMapface` they lift and desaturate into a pale sheet, which is
//! what this sea was.

use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::math::DVec3;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use freeport_core::dc::DcMesh;
use freeport_core::water::SURFACE;

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExt>;

/// Radial displacement bound for `water_lib.wgsl::swell`. Its three sine
/// weights sum to 0.36; amplitude comes from the actual material uniform.
pub fn swell_bound(water: &WaterExt) -> f32 {
    (0.18 + 0.12 + 0.06) * water.wave.w.abs()
}

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
    /// The sun's direction in xyz, written every frame by `sky.rs`, and the
    /// NIGHT FLOOR in w: what a share of the light reaching the water's own
    /// body is worth on the half of the planet the sun is not on.
    #[uniform(100)]
    pub sun: Vec4,
    /// Absorption per metre of water, a channel each, and in w the longest
    /// path any of it is measured over.
    #[uniform(100)]
    pub absorb: Vec4,
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

/// A mark on the sea's chunks, beside their `Anchored`.
#[derive(Component)]
pub struct Sheet;

/// The sheet's numbers, tenebris's `water.yaml`, in one place, so nothing
/// that draws the sea spells them itself.
pub fn sheet_ext(sea: f64) -> WaterExt {
    let nits = freeport_core::atmos::NITS as f32;
    WaterExt {
        centre: Vec4::new(0.0, 0.0, 0.0, sea as f32),
        // Steepness 0.45 and the slope cap 0.7 are the calmed waves of
        // pale-blue-dot's second round; this sheet was at 0.65 and 1.6,
        // which is a chop the ripple fade then turned into sparkle.
        wave: Vec4::new(0.75, RIPPLE, 0.45, 0.5),
        // NO RED AT ALL, which is the one thing a saturated sea needs
        // under a tone mapper: tenebris's 0.02 is worth ten levels on
        // screen and reads as grey. This is also the colour the underwater
        // view saturates to, so it is the one place the sea's body is
        // written down.
        deep: Vec4::new(0.0, 0.12, 0.28, 2.0),
        // The three the shader takes through the camera's exposure are in
        // CANDELA, `atmos::NITS` times the colour they are authored as, or
        // they arrive at five ten thousandths of the ground beside them
        // and the sheet reflects nothing at all.
        //
        // A clear day blue rather than tenebris's near white, and the
        // fresnel floor 0.22 rather than 0.5: the floor is the single knob
        // with measurable authority over deep water at a grazing angle
        // (22 levels of red on pale-blue-dot's wade frame) and the near
        // white was what the tone mapper turned into a pale sheet.
        horizon: (Vec3::new(0.10, 0.36, 0.72) * nits).extend(0.22),
        zenith: (Vec3::new(0.03, 0.18, 0.55) * nits).extend(0.7),
        foam: (Vec3::new(0.95, 0.97, 1.0) * nits).extend(0.10),
        band: Vec4::new(0.35, 0.60, 0.50, 1.20),
        fog: Vec4::ZERO,
        haze: Vec4::ZERO,
        // The direction is written every frame by `sky.rs`; the floor is
        // pale-blue-dot's `night_floor`.
        sun: Vec4::new(0.0, 1.0, 0.0, NIGHT_FLOOR),
        absorb: ABSORB.extend(MAX_PATH),
    }
}

/// Absorption per metre of sea water, a channel each: pale-blue-dot's
/// `absorption_per_m`, and the term with more authority over the colour of
/// this sheet than every shine knob in the shader put together. Red is
/// hardest because the sand under a metre of water is red and only the
/// water in front of it can take that out.
pub const ABSORB: Vec3 = Vec3::new(0.90, 0.25, 0.08);

/// The longest path of water any of it is measured over, metres. Past this
/// the sheet is its own deep colour and nothing behind it is seen.
pub const MAX_PATH: f32 = 200.0;

/// What a share of the light reaching the water's own body is worth on the
/// night side. A floor rather than nought, which is this project's own
/// ambient lesson: nought is a hole in the picture rather than a sea.
pub const NIGHT_FLOOR: f32 = 0.18;

/// Bevy's own transmission attenuates the refracted ray by
/// `attenuation_color ^ (thickness / attenuation_distance)`, so at a
/// distance of one metre the colour it wants IS `exp(-absorption)`. It is
/// DERIVED here rather than authored beside `ABSORB`, because two numbers
/// that have to agree are one number: the shader reads `ABSORB` for the
/// deep colour the path runs out into, and Bevy reads this for the same
/// Beer's law over the same capped path.
pub fn attenuation(absorb: Vec3) -> Color {
    Color::linear_rgb((-absorb.x).exp(), (-absorb.y).exp(), (-absorb.z).exp())
}

/// The standard material under the sheet, likewise shared: transmissive,
/// and attenuating over the water it is looked through by the SAME
/// absorption the extension carries.
///
/// The roughness and the reflectance are the sun's glint, which is the one
/// thing this keeps on Bevy's own PBR rather than adding a second
/// highlight of its own: pale-blue-dot measured its explicit specular at
/// ONE level out of 255, so a second one would be a term nobody can see
/// drawn twice. 0.02 is water's real F0, which is Bevy's `reflectance`
/// 0.25, and 0.12 of roughness is a sea rather than the mirror 0.06 was.
pub fn sheet_base() -> StandardMaterial {
    StandardMaterial {
        // The diffuse eighth the transmission leaves is the sea's own body
        // colour, so it is the deep colour and not a second blue.
        base_color: Color::linear_rgb(0.0, 0.12, 0.28),
        perceptual_roughness: 0.12,
        metallic: 0.0,
        reflectance: 0.25,
        specular_transmission: 0.92,
        ior: 1.33,
        thickness: 2.0,
        attenuation_distance: 1.0,
        attenuation_color: attenuation(ABSORB),
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

/// The ripple coordinate's scale, noise cells a metre, and the only place
/// it is written down: `sheet_ext` hands it to the shader in `wave.y` and
/// this works the cell out with it in `f64`.
///
/// Two thirds of a metre a cell, which is what a ripple on this sea is.
pub const RIPPLE: f32 = 1.35;

/// A chunk's sea surface as a Bevy mesh: the surface triangles only,
/// vertices shared, normals the field's. None if nothing is drawn.
///
/// Every vertex also carries the chunk's own RIPPLE CELL and the fraction
/// inside it, worked out here in `f64` from the chunk's planet relative
/// corner. The shader adds the vertex's own offset and doubles the pair
/// exactly per octave, so the sea's noise is continuous over a body two
/// thousand kilometres across at the precision of the fraction. What it
/// replaces is `world_position - centre` formed in the fragment out of a
/// planet's radius held in one float: 6.25 cm steps on 0.67 m features,
/// which is the pixelation.
pub fn to_sheet(m: &DcMesh, corner: DVec3) -> Option<Mesh> {
    let mut indices: Vec<u32> = Vec::new();
    for (t, material) in m.indices.chunks(3).zip(&m.materials) {
        if *material == SURFACE {
            indices.extend_from_slice(t);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let base = corner * RIPPLE as f64;
    let cell = base.floor();
    let frac = (base - cell).as_vec3();
    // A cell is an exact whole number in an f32 up to sixteen million, and
    // a thousand kilometre planet at this scale reaches 1.35 million, which
    // the shader's own two doublings take to 5.4 million.
    let cell = cell.as_vec3();
    let n = m.positions.len();
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            vec![[cell.x, cell.y, cell.z, 1.0]; n],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[frac.x, frac.y]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[frac.z, 0.0]; n])
        .with_inserted_indices(bevy::mesh::Indices::U32(indices)),
    )
}

/// Keep the sea's centre with the origin, as the terrain's is.
pub fn recentre(
    materials: &mut Assets<WaterMaterial>,
    handle: &Handle<WaterMaterial>,
    centre: Vec3,
) {
    if materials
        .get(handle)
        .is_none_or(|m| m.extension.centre.truncate() == centre)
    {
        return;
    }
    if let Some(m) = materials.get_mut(handle) {
        m.extension.centre = centre.extend(m.extension.centre.w);
    }
}
