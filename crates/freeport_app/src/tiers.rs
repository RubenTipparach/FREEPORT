//! The two tiers of a hex world, and both are generated in the VERTEX
//! stage: nothing about this geometry is built on the CPU or uploaded.
//!
//! The near tier is Goldberg columns (`freeport_core::hex` addressed in
//! `tiers.wgsl`), the far tier is sp4cerat's Planet-LOD triangles
//! (`freeport_core::lod`) tessellated on the way past, and the two meet at
//! a disc: the hex tier draws a disc of tiles round the eye, and `select`
//! is given that disc as a HOLE so the far tier draws nothing wholly
//! inside it. What the eye sees at the join is the columns' own sides.
//!
//! **Why the vertex stage and not a compute pass.** A compute pass that
//! wrote this geometry would have to write it to memory, wait on a
//! barrier, and read it back in as a vertex buffer, and the only thing
//! that buys is the geometry being READABLE afterwards: an indirect count,
//! a second pass over it, a physics query. Nothing here wants that. The
//! vertex stage generates the same triangles with no buffer, no barrier
//! and no round trip, which is strictly less work for the same picture.
//! A compute pass is the right answer the day the count has to be known on
//! the GPU (a real indirect draw) or the day a second pass reads the
//! triangles, and the arithmetic in `field.wgsl` is written to be called
//! from either.
//!
//! **What IS on the CPU: one `select` a frame.** Planet-LOD's recursion
//! picks the leaves, a few thousand of them, and they go up as a storage
//! buffer of corners; the vertex stage then makes `sub * sub` triangles
//! out of every leaf. So the CPU hands over a few thousand triangles and
//! the GPU draws a hundred thousand, and the amplification is the number
//! the design rests on. The recursion cannot be a vertex shader (a vertex
//! shader emits one vertex, not a variable number of triangles), and a
//! compute recursion is what this becomes when `select` stops being free.

use crate::terrain::FRAMES;
use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::math::DVec3;
use bevy::mesh::{MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;
use freeport_core::{hex, lod};

pub type TierMaterial = ExtendedMaterial<StandardMaterial, Tier>;
pub type SeaMaterial = ExtendedMaterial<StandardMaterial, SeaTier>;

/// Which tier a material draws. It is the whole of the pipeline key,
/// because the two tiers differ in exactly one thing: which entry point of
/// `tiers.wgsl` makes their vertices.
#[repr(u32)]
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Default, Reflect)]
pub enum Which {
    #[default]
    Lod,
    Hex,
    /// The sheet, which is neither tier and both: one surface at the sea's
    /// radius over the far tier's leaves, with no hole cut in it, so it
    /// lies over the hex columns at a shore exactly as it lies over the
    /// far tier's triangles. A sea is flat whatever the ground under it is
    /// made of.
    Sea,
}

/// The lanes every tier's VERTEX stage reads, as one uniform at binding
/// 110. It is a `ShaderType` of its own rather than eight fields on each
/// material, because the sea wears `water.wgsl`'s bindings at 100 and the
/// ground wears `terrain.wgsl`'s, so the two materials cannot be one type
/// and their shared half must not be written twice: this is the half, in
/// one place, and `tiers.wgsl`'s `Tier` struct is its transcription.
#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct Lanes {
    /// The planet's centre in the render frame, w: how many leaves of the
    /// storage buffer are live, which is where the far tier and the sea
    /// stop. The centre is here as well as in binding 100 because one
    /// vertex shader serves three materials and binding 100 is a different
    /// struct under each, so it can name none of them.
    pub at: Vec4,
    /// The ANCHOR: one unit direction, the eye's own tile's, that every
    /// vertex of every tier is an offset from. In w, the far tier's hole,
    /// as the SQUARED CHORD of the angle it reaches rather than its
    /// cosine, because a cosine near one is a number an f32 cannot tell
    /// from one.
    pub disc: Vec4,
    /// Where the anchor's own ground stands in the RENDER frame,
    /// `anchor * radius + centre` worked out in f64: the one large
    /// subtraction in the whole draw, done once and where it can be done
    /// exactly. Every vertex is this plus a small accurate offset, which
    /// is how a planet a thousand kilometres across is drawn out of f32
    /// (`freeport_core::pos::unit_offset`).
    pub base: Vec4,
    /// x: mean radius, y: the sea's radius, z: peak to trough of the
    /// relief, w: how many relief features fit round the planet.
    pub shape: Vec4,
    /// x: octaves of relief, y: the seed, z: how many pieces a leaf's edge
    /// is cut into, w: tiles along an icosahedron edge, the hex grid's `n`.
    pub counts: UVec4,
    /// The lattice's first step off that anchor, w: how deep a column's
    /// skirt hangs, metres.
    pub lat1: Vec4,
    /// Its second step, w: how many tiles the hex window reaches.
    pub lat2: Vec4,
    /// `water::WaterExt`'s `wave` and `deep`, so the sea tier lifts its
    /// vertices by the same swell the dual contoured sheet does. Nought on
    /// a ground tier, which never asks.
    pub wave: Vec4,
    pub deep: Vec4,
}

/// A GROUND tier: bindings 100 to 106 are `terrain::Terrain`'s to the
/// field, because the fragment shader IS `terrain.wgsl` and a bind group
/// layout is what a shader is compiled against: if the two ever drift, the
/// pipeline fails to build rather than drawing something wrong.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(Which)]
pub struct Tier {
    /// x: metres a tile on the ground, y: on concrete, z: how many town
    /// frames are set, w: the sea's radius.
    #[uniform(100)]
    pub params: Vec4,
    /// The planet's centre in the render frame.
    #[uniform(100)]
    pub centre: Vec4,
    /// Town frames, as `terrain::Terrain::frames`.
    #[uniform(100)]
    pub frames: [Vec4; FRAMES * 3],
    /// The sky at the horizon and its density, as `terrain::Terrain::fog`.
    #[uniform(100)]
    pub fog: Vec4,
    /// The ground fog's shape, as `terrain::Terrain::haze`.
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
    #[uniform(110)]
    pub lanes: Lanes,
    /// Planet-LOD's leaves, three corners a triangle, as directions.
    #[storage(111, read_only)]
    pub leaves: Handle<ShaderStorageBuffer>,
    /// Which tier: the key, and never read by a shader.
    pub which: Which,
}

/// The SEA tier: binding 100 is `water::WaterExt`'s, because the fragment
/// shader is `water.wgsl`, tenebris's own. The terrain's textures are not
/// here because the sheet samples none of them, and the lanes at 110 are
/// the same `Lanes` the ground tiers carry, so one vertex shader compiles
/// against both layouts.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(Which)]
pub struct SeaTier {
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
    #[uniform(110)]
    pub lanes: Lanes,
    #[storage(111, read_only)]
    pub leaves: Handle<ShaderStorageBuffer>,
    /// Always `Which::Sea`, and the key that picks the entry point.
    pub which: Which,
}

impl From<&SeaTier> for Which {
    fn from(tier: &SeaTier) -> Which {
        tier.which
    }
}

impl MaterialExtension for SeaTier {
    fn vertex_shader() -> ShaderRef {
        "embedded://freeport_app/tiers.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/water.wgsl".into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<SeaTier>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.entry_point = Some("sea".into());
        // The sheet carries the water's own COLUMN under each vertex in
        // `uv.x` and `water.wgsl` reads it there, so both stages are told
        // the varying exists. The mesh has no such attribute and does not
        // need one: `VertexOutput` is what the define shapes, and what the
        // vertex stage reads is still position alone.
        let defs = ["VERTEX_UVS", "VERTEX_UVS_A", "WATER_COLUMN"];
        for def in defs {
            descriptor.vertex.shader_defs.push(def.into());
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push(def.into());
            }
        }
        Ok(())
    }
}

impl From<&Tier> for Which {
    fn from(tier: &Tier) -> Which {
        tier.which
    }
}

impl MaterialExtension for Tier {
    fn vertex_shader() -> ShaderRef {
        "embedded://freeport_app/tiers.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/terrain.wgsl".into()
    }

    /// Neither tier is in the depth prepass or the shadow pass, because
    /// both of those draw with Bevy's own vertex shader and would draw the
    /// undisplaced dummy mesh: a depth buffer of a point at the origin and
    /// a shadow map of nothing. Putting them back means transcribing the
    /// prepass vertex stage too, and that is what it will take.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Tier>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.entry_point = Some(
            match key.bind_group_data {
                Which::Lod => "lod",
                Which::Hex => "hex",
                Which::Sea => "sea",
            }
            .into(),
        );
        // A tier's triangles can be kilometres across, so the height over
        // the sea rides DOWN from the vertex stage rather than being
        // worked out again from the fragment's interpolated position,
        // which on a far leaf is a chord under the sphere. The mesh has no
        // uv attribute and does not need one: `VertexOutput` is what the
        // define shapes.
        let defs = ["VERTEX_UVS", "VERTEX_UVS_A", "TIER_HEIGHT"];
        for def in defs {
            descriptor.vertex.shader_defs.push(def.into());
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push(def.into());
            }
        }
        Ok(())
    }
}

pub struct TiersPlugin;

/// Handles held so the imported libraries are LOADED rather than merely
/// registered. A shader nothing has asked for is not in the asset system,
/// and an import that is not in the asset system is a pipeline Bevy
/// quietly retries for ever: this has now drawn an empty sky twice with
/// not one error in the log, once for `freeport::field` and once for
/// `freeport::frame`.
#[derive(Resource)]
struct Libraries(#[allow(dead_code)] Vec<Handle<Shader>>);

impl Plugin for TiersPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "field.wgsl");
        embedded_asset!(app, "frame.wgsl");
        embedded_asset!(app, "tiers.wgsl");
        let assets = app.world().resource::<AssetServer>();
        let held = ["field.wgsl", "frame.wgsl"]
            .iter()
            .map(|name| assets.load(format!("embedded://freeport_app/{name}")))
            .collect();
        app.insert_resource(Libraries(held));
        app.add_plugins((
            MaterialPlugin::<TierMaterial>::default(),
            MaterialPlugin::<SeaMaterial>::default(),
        ));
    }
}

/// A mesh of `verts` vertices whose position carries its own NUMBER and
/// nothing else: every tier makes its real position out of that number and
/// the uniforms.
///
/// The number is in the mesh rather than read off `@builtin(vertex_index)`
/// because Bevy packs every mesh into one shared vertex buffer and draws a
/// slice of it, so the built in index is where the vertex sits in the
/// SLAB. The tier allocated first counted from nought and drew; the tier
/// after it counted from a million and drew nothing at all, with no error
/// anywhere, because every one of its vertices worked out a tile outside
/// the window. A number that rides the mesh is the mesh's own.
pub fn counted_mesh(verts: usize) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let places: Vec<[f32; 3]> = (0..verts).map(|i| [i as f32, 0.0, 0.0]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, places);
    mesh
}

/// What a tier is drawn as: one entity, one mesh, one material, never
/// moved and never culled, since where its triangles are is a thing only
/// its vertex shader knows.
pub fn spawn_tier(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<TierMaterial>,
) -> Entity {
    commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            NoFrustumCulling,
        ))
        .id()
}

/// What the two tiers are drawn at. The near tier's disc is bounded and
/// the far tier's hole is that same disc, so the two never both draw a
/// patch of ground and never both leave one out.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Tiers {
    /// The planet, as the shaders take it.
    pub radius: f64,
    pub sea: f64,
    pub relief: f64,
    pub lumps: f64,
    pub octaves: u32,
    pub seed: u32,
    /// Metres a hex tile, and how many tiles the window reaches.
    pub tile: f64,
    pub span: u32,
    /// How deep a column's skirt hangs, metres.
    pub skirt: f64,
    /// Planet-LOD's quality knob, and how many pieces a leaf's edge is cut
    /// into on the way past: the detail on the ground is the product.
    pub ratio: f64,
    pub sub: u32,
}

impl Tiers {
    /// The grid the near tier's tiles come off.
    pub fn grid(&self) -> hex::Grid {
        hex::Grid::for_tile(self.radius, self.tile)
    }

    /// How far the hex disc reaches, radians. The window is a disc in the
    /// lattice's own norm, so its reach is its span in tiles.
    pub fn disc(&self) -> f64 {
        self.grid().spacing(self.radius) * self.span as f64 / self.radius
    }

    /// How many leaves the far tier's buffer and mesh are built for. The
    /// count `select` returns moves with the eye, so the mesh is sized for
    /// the worst of it and the vertex shader says nothing past the live
    /// count.
    pub fn most_leaves(&self) -> usize {
        MOST_LEAVES
    }

    /// Vertices the far tier's blank mesh needs: three a sub triangle,
    /// `sub * sub` of them a leaf.
    pub fn lod_verts(&self) -> usize {
        self.most_leaves() * (self.sub * self.sub) as usize * 3
    }

    /// Vertices the near tier's: a prism a tile over the whole square
    /// window, the disc cut out of it in the shader.
    pub fn hex_verts(&self) -> usize {
        let wide = (self.span * 2 + 1) as usize;
        wide * wide * PRISM_VERTS
    }
}

/// A prism's triangles as `tiers.wgsl` lays them out: four across the top
/// and two down each of six sides.
const PRISM_VERTS: usize = 48;
/// How many tiles of the near tier's disc the far tier is drawn under.
const OVERLAP: f64 = 6.0;
/// How far toward the eye the hex tier's depth is nudged, so the columns
/// beat the far tier's surface wherever the two are drawn over one another.
const HEX_BIAS: f32 = 2000.0;
/// Leaves the far tier is built for. Planet-LOD at ratio 6 picks 4,147
/// from the ground of a 10 km planet and 8,396 from a 1,000 km one
/// (`lod::sizes::the_cost_of_a_bigger_planet`, which is the sweep this
/// number is read off), so a cap of 8,192 was UNDER the planet the
/// harness now runs and the overflow is a hole in the ground that says
/// nothing. Half again over the measured worst is the slack, and it is
/// not more than that because every vertex the mesh carries past the live
/// count still runs the vertex stage far enough to work out it has
/// nothing to say.
const MOST_LEAVES: usize = 12_288;

/// The asset stores the tiers write into, as one thing: a system that
/// reaches for four of them is a system with four arguments, and the
/// spawn and the feed both want the same four.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Store<'w> {
    pub images: ResMut<'w, Assets<Image>>,
    pub materials: ResMut<'w, Assets<TierMaterial>>,
    pub seas: ResMut<'w, Assets<SeaMaterial>>,
    pub buffers: ResMut<'w, Assets<ShaderStorageBuffer>>,
}

/// Both tiers, their meshes, their materials and the buffer the far tier's
/// leaves ride in.
#[derive(Resource)]
pub struct Drawn {
    pub near: Handle<TierMaterial>,
    pub far: Handle<TierMaterial>,
    pub sea: Handle<SeaMaterial>,
    pub leaves: Handle<ShaderStorageBuffer>,
}

/// The three entities and everything they need: the two ground tiers and
/// the sheet over them.
pub fn spawn_tiers(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &mut Store,
    at: &Tiers,
) {
    let drawn = tier_materials(assets, at);
    spawn_tier(
        commands,
        meshes.add(counted_mesh(at.lod_verts())),
        drawn.far.clone(),
    );
    spawn_tier(
        commands,
        meshes.add(counted_mesh(at.hex_verts())),
        drawn.near.clone(),
    );
    // The sheet rides the far tier's own leaves, so its mesh is the same
    // size: the sea is drawn on the same triangles at a different radius.
    commands.spawn((
        Mesh3d(meshes.add(counted_mesh(at.lod_verts()))),
        MeshMaterial3d(drawn.sea.clone()),
        Transform::IDENTITY,
        NoFrustumCulling,
    ));
    info!(
        "tiers: hex at {:.2} m tiles ({} of them round the planet), a disc of {} tiles and {:.0} m, {} prisms of {} triangles; Planet-LOD at ratio {} cut {} ways, {} vertices",
        at.grid().spacing(at.radius),
        at.grid().count(),
        at.span,
        at.disc() * at.radius,
        at.hex_verts() / PRISM_VERTS,
        at.hex_verts() / 3,
        at.ratio,
        at.sub,
        at.lod_verts(),
    );
    commands.insert_resource(drawn);
}

/// The three materials and the buffer their leaves ride in: two ground
/// tiers wearing `terrain.wgsl` and the sheet wearing `water.wgsl`, all
/// three reading one `Lanes` and one vertex shader.
fn tier_materials(assets: &mut Store, at: &Tiers) -> Drawn {
    let Store {
        images,
        materials,
        seas,
        buffers,
    } = assets;
    let maps = crate::terrain::terrain_maps(images);
    let (ground_tile, concrete_tile) = crate::terrain::tiles();
    let leaves = buffers.add(ShaderStorageBuffer::from(vec![
        Vec4::ZERO;
        at.most_leaves() * 3
    ]));
    let sheet = crate::water::sheet_ext(at.sea);
    let lanes = Lanes {
        at: Vec4::ZERO,
        disc: Vec4::ZERO,
        base: Vec4::ZERO,
        shape: Vec4::new(
            at.radius as f32,
            at.sea as f32,
            at.relief as f32,
            at.lumps as f32,
        ),
        counts: UVec4::new(at.octaves, at.seed, at.sub, at.grid().n),
        lat1: Vec4::new(0.0, 0.0, 0.0, at.skirt as f32),
        lat2: Vec4::new(0.0, 0.0, 0.0, at.span as f32),
        wave: sheet.wave,
        deep: sheet.deep,
    };
    let make = |which: Which| Tier {
        params: Vec4::new(ground_tile, concrete_tile, 0.0, at.sea as f32),
        centre: Vec4::ZERO,
        frames: [Vec4::ZERO; FRAMES * 3],
        fog: Vec4::ZERO,
        haze: Vec4::ZERO,
        albedo: maps[0].clone(),
        normal: maps[1].clone(),
        orm: maps[2].clone(),
        lanes,
        leaves: leaves.clone(),
        which,
    };
    // Neither tier is culled by its winding: which way round a hexagon's
    // corners come out depends on the handedness of the lattice basis it
    // was stepped along, and a normal that points out is cheaper to
    // guarantee than a winding that does.
    let base = |bias: f32| StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        cull_mode: None,
        double_sided: true,
        depth_bias: bias,
        ..default()
    };
    let far = materials.add(ExtendedMaterial {
        base: base(0.0),
        extension: make(Which::Lod),
    });
    // Where the two ground tiers overlap they carry the same height
    // differently, so which one a pixel gets is a coin toss on a half
    // metre: the columns must win it every time, or the far tier's
    // smooth surface pokes through the terraces along the whole rim.
    // `HEX_BIAS` is what settles it, in the depth test rather than in
    // the geometry, so nothing has to be moved to say it.
    let near = materials.add(ExtendedMaterial {
        base: base(HEX_BIAS),
        extension: make(Which::Hex),
    });
    let sea = seas.add(ExtendedMaterial {
        base: crate::water::sheet_base(),
        extension: SeaTier {
            centre: sheet.centre,
            wave: sheet.wave,
            deep: sheet.deep,
            horizon: sheet.horizon,
            zenith: sheet.zenith,
            foam: sheet.foam,
            band: sheet.band,
            fog: sheet.fog,
            haze: sheet.haze,
            lanes,
            leaves: leaves.clone(),
            which: Which::Sea,
        },
    });
    Drawn {
        near,
        far,
        sea,
        leaves,
    }
}

/// The leaves Planet-LOD picked, into the buffer the far tier and the sea
/// both read, as OFFSETS from the anchor: the subtraction is done in f64
/// here so the shader never has to form a unit vector at a planet's
/// scale. A far leaf's offset is large and its accuracy does not matter;
/// a near one's is small and exact. Answers how many went, which is where
/// the shader is told to stop.
fn send_leaves(
    at: &Tiers,
    drawn: &Drawn,
    buffers: &mut Assets<ShaderStorageBuffer>,
    anchor: DVec3,
    picked: &[lod::Tri],
) -> usize {
    let live = picked.len().min(at.most_leaves());
    if let Some(buffer) = buffers.get_mut(&drawn.leaves) {
        let mut data: Vec<Vec4> = Vec::with_capacity(live * 3);
        for tri in picked.iter().take(live) {
            for corner in tri {
                data.push((*corner - anchor).as_vec3().extend(0.0));
            }
        }
        data.resize(at.most_leaves() * 3, Vec4::ZERO);
        buffer.set_data(data.as_slice());
    }
    live
}

/// Every frame: where the eye is, which tile it stands on, and which
/// leaves Planet-LOD picks from there. This is the whole of what the CPU
/// does for either tier.
pub fn feed_tiers(
    eye: Res<crate::Eye>,
    frame: Res<crate::stream::Frame>,
    at: Res<Tiers>,
    drawn: Res<Drawn>,
    mut assets: Store,
    mut said: Local<usize>,
) {
    let clock = std::time::Instant::now();
    let here = eye.0 .0;
    let centre = frame.0.local(freeport_core::pos::WorldPos(DVec3::ZERO));
    let dir = here.normalize_or(DVec3::Z);
    let grid = at.grid();
    // The anchor: the eye's own tile, as a point in its face's plane and
    // as the unit direction every vertex of every tier is measured off.
    // The two lattice steps come down DIVIDED by that point's own length,
    // so what the shader adds to a unit anchor is a small number.
    let (point, e1, e2) = grid.basis(grid.at(dir));
    let span = point.length().max(f64::MIN_POSITIVE);
    let anchor = point / span;
    let (e1, e2) = (e1 / span, e2 / span);
    // The one large subtraction, in f64: where the anchor's own ground
    // stands in the render frame. Everything else is an offset from it.
    let base = (anchor * at.radius - frame.0.at).as_vec3();
    // `select` is asked for the WHOLE planet, with no hole: the far tier
    // cuts its own in the shader, a sub triangle at a time, and the sea
    // rides the very same leaves with no hole at all. One walk a frame
    // serves all three.
    //
    // The hole is the near tier's disc less `OVERLAP` tiles, so the two
    // ground tiers OVERLAP at the rim rather than meeting there. A tile of
    // overlap was not enough and the picture said so: at a grazing angle
    // the rim was a band of SKY, because the two carry the same height
    // differently (a column's top is flat at its middle's height, a leaf's
    // is linear between its corners) and where the far tier stood higher
    // the line of sight went under it, over the ground behind and out.
    // The hole, as the SQUARED CHORD of its angle: `|dir - anchor|^2` is
    // `2 (1 - cos t)`, and a chord is where an f32 keeps its precision
    // while a cosine near one is a number it cannot tell from one.
    let reach = at.disc() - OVERLAP * grid.spacing(at.radius) / at.radius;
    let hole = 2.0 * (1.0 - reach.cos());
    let picked = lod::select(
        here,
        at.radius,
        &lod::Lod {
            ratio: at.ratio,
            detail: 0.0,
            cull: true,
        },
        0.0,
        dir,
    );
    let live = send_leaves(&at, &drawn, &mut assets.buffers, anchor, &picked);
    // What the CPU costs, said when it moves by a fifth: the whole of the
    // far tier's work is this one `select`, and the vertex stage makes
    // `sub * sub` triangles out of every leaf it picks.
    if picked.len() * 5 > *said * 6 || picked.len() * 6 < *said * 5 {
        // What was PICKED throttles the line, not what was drawn, so a
        // truncated frame does not pin the count and report itself for
        // ever. A truncation is a piece of the planet not drawn, so it
        // says so rather than leaving a hole for somebody to find in a
        // picture.
        if picked.len() > live {
            warn!(
                "Planet-LOD picked {} leaves and the buffer holds {live}: raise MOST_LEAVES",
                picked.len(),
            );
        }
        info!(
            "Planet-LOD picked {live} leaves in {:.2} ms, drawn as {} triangles",
            clock.elapsed().as_secs_f64() * 1e3,
            live * (at.sub * at.sub) as usize,
        );
        *said = picked.len();
    }
    // The anchor doubles as the hex disc's own middle, which is what the
    // far tier measures its hole against, so the near tier and the hole
    // are one direction and never two.
    let step = |count: usize, lanes: &mut Lanes| {
        lanes.at = centre.extend(count as f32);
        lanes.disc = anchor.as_vec3().extend(hole as f32);
        lanes.base = base.extend(0.0);
        lanes.lat1 = e1.as_vec3().extend(at.skirt as f32);
        lanes.lat2 = e2.as_vec3().extend(at.span as f32);
    };
    for handle in [&drawn.far, &drawn.near] {
        if let Some(m) = assets.materials.get_mut(handle) {
            m.extension.centre = centre.extend(0.0);
            step(live, &mut m.extension.lanes);
        }
    }
    if let Some(m) = assets.seas.get_mut(&drawn.sea) {
        m.extension.centre = centre.extend(m.extension.centre.w);
        step(live, &mut m.extension.lanes);
    }
}

/// The two tiers' entities, once `spawn_world` has put `Tiers` in place.
pub fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut assets: Store,
    at: Res<Tiers>,
) {
    spawn_tiers(&mut commands, &mut meshes, &mut assets, &at);
}
