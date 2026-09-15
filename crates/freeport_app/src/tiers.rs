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
use freeport_core::hex;

pub type TierMaterial = ExtendedMaterial<StandardMaterial, Tier>;
pub type SeaMaterial = ExtendedMaterial<StandardMaterial, SeaTier>;

/// The towns as `terrain.wgsl` takes them: three lanes a town and how
/// many of them, which is what `terrain::frame_lanes` makes of a list of
/// frames. It is a value rather than a list so `Tiers` stays `Copy`.
#[derive(Clone, Copy, Debug)]
pub struct TownLanes {
    pub lanes: [Vec4; FRAMES * 3],
    pub count: f32,
}

/// Which tier a material draws. It is the whole of the pipeline key,
/// because the two tiers differ in exactly one thing: which entry point of
/// `tiers.wgsl` makes their vertices.
#[repr(u32)]
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Default, Reflect)]
pub enum Which {
    #[default]
    Lod,
    Hex,
    /// The sheet past the hex disc: one surface at the sea's radius over
    /// the far tier's leaves, with the disc cut out of it exactly as the
    /// far GROUND tier has it cut out.
    Sea,
    /// The sea inside the disc, as COLUMNS: a prism of water on every tile
    /// whose ground stands under the sea's level, flat on top at that
    /// level and hanging a skirt below it, so a shore is a wall of water
    /// down to the beach rather than a sheet fading into it. The owner's
    /// ask, and tenebris's water: "hex based water, so we can have voxel
    /// water".
    HexSea,
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
    /// What every tile of the hex window carries: how far it has been
    /// RAISED in x and what it is MADE OF in y, in the window's own order,
    /// so a shader indexes it with the number it already has worked out.
    #[storage(112, read_only)]
    pub raised: Handle<ShaderStorageBuffer>,
    /// The towns' levelled ground, two lanes a site, which `field.wgsl`
    /// owns and `send_sites` fills.
    #[storage(113, read_only)]
    pub sites: Handle<ShaderStorageBuffer>,
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
    /// What every tile of the hex window carries: its rise in x and its
    /// material in y.
    #[storage(112, read_only)]
    pub raised: Handle<ShaderStorageBuffer>,
    /// The towns' levelled ground, two lanes a site.
    #[storage(113, read_only)]
    pub sites: Handle<ShaderStorageBuffer>,
    /// `Which::Sea` or `Which::HexSea`, and the key that picks the entry
    /// point.
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
        key: MaterialExtensionKey<SeaTier>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.entry_point = Some(
            match key.bind_group_data {
                Which::HexSea => "hexsea",
                _ => "sea",
            }
            .into(),
        );
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
        // A `Tier` is a GROUND tier and carries only the two: the sea's
        // two are `SeaTier`'s, which picks its own entry point off the
        // same key.
        descriptor.vertex.entry_point = Some(
            match key.bind_group_data {
                Which::Hex => "hex",
                _ => "lod",
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
    /// Every town's own frame as the shader takes it, three lanes a town
    /// and how many, off `terrain::frame_lanes`. `terrain.wgsl` maps
    /// concrete, plate and a street in the nearest one, because a wall
    /// plumb on its lot has constant east or north up its height only in
    /// the frame of the town it stands in: in the planet's own the panel
    /// seams run across it at whatever angle the two make, which is what
    /// the owner read off the first render of a city.
    pub towns: TownLanes,
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
        self.window() * PRISM_VERTS
    }

    /// Tiles in the near tier's square window, which is what the raised
    /// buffer holds one of each: the shader's own `tile` number indexes
    /// both.
    pub fn window(&self) -> usize {
        let wide = (self.span * 2 + 1) as usize;
        wide * wide
    }
}

/// A prism's triangles as `tiers.wgsl` lays them out: four across the top
/// and two down each of six sides.
const PRISM_VERTS: usize = 48;
/// How many tiles of the near tier's disc the far tier is drawn under.
pub const OVERLAP: f64 = 6.0;
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
    /// The sea inside the hex disc, as columns.
    pub shallows: Handle<SeaMaterial>,
    pub leaves: Handle<ShaderStorageBuffer>,
    /// The hex window's raised heights, and which anchor and which edit
    /// they were filled for, so a frame that changed neither writes
    /// nothing.
    pub raised: Handle<ShaderStorageBuffer>,
    pub filled: Option<(hex::Tile, u64)>,
    /// The towns' sites as offsets from the anchor, and the anchor they
    /// were differenced against, so they are redone when it moves and not
    /// once a frame.
    pub sites: Handle<ShaderStorageBuffer>,
    pub sited: Option<hex::Tile>,
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
    // And the sea inside the disc rides the hex tier's mesh, because a
    // water column is the same prism a ground column is.
    commands.spawn((
        Mesh3d(meshes.add(counted_mesh(at.hex_verts()))),
        MeshMaterial3d(drawn.shallows.clone()),
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
    let (leaves, raised, sites) = tier_buffers(buffers, at);
    let sheet = crate::water::sheet_ext(at.sea);
    let lanes = lanes_of(at, &sheet);
    let make = |which: Which| Tier {
        params: Vec4::new(ground_tile, concrete_tile, at.towns.count, at.sea as f32),
        centre: Vec4::ZERO,
        frames: at.towns.lanes,
        fog: Vec4::ZERO,
        haze: Vec4::ZERO,
        albedo: maps[0].clone(),
        normal: maps[1].clone(),
        orm: maps[2].clone(),
        lanes,
        leaves: leaves.clone(),
        raised: raised.clone(),
        sites: sites.clone(),
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
    // The columns win the depth test where the two seas overlap, for the
    // reason the ground tiers' own overlap has: the overlap exists so the
    // near tier covers the far one and never the other way about.
    let sea = sea_material(
        seas,
        &sheet,
        lanes,
        &leaves,
        &raised,
        &sites,
        Which::Sea,
        0.0,
    );
    let shallows = sea_material(
        seas,
        &sheet,
        lanes,
        &leaves,
        &raised,
        &sites,
        Which::HexSea,
        HEX_BIAS,
    );
    Drawn {
        near,
        far,
        sea,
        shallows,
        leaves,
        raised,
        filled: None,
        sites,
        sited: None,
    }
}

/// The three storage buffers `feed` writes: Planet-LOD's leaves, what has
/// been built on the hex tiles, and the towns' levelled sites. The sites
/// are never NOUGHT lanes, because a zero length storage buffer is not a
/// binding: with no towns this is one site whose skirt runs from two
/// metres BEHIND the viewer to one metre behind, which weighs nought at
/// every distance a point can be and costs the loop one iteration rather
/// than costing every caller a branch.
fn tier_buffers(
    buffers: &mut Assets<ShaderStorageBuffer>,
    at: &Tiers,
) -> (
    Handle<ShaderStorageBuffer>,
    Handle<ShaderStorageBuffer>,
    Handle<ShaderStorageBuffer>,
) {
    (
        buffers.add(ShaderStorageBuffer::from(vec![
            Vec4::ZERO;
            at.most_leaves() * 3
        ])),
        buffers.add(ShaderStorageBuffer::from(vec![Vec2::ZERO; at.window()])),
        buffers.add(ShaderStorageBuffer::from(vec![
            Vec4::ZERO,
            Vec4::new(-2.0, -1.0, 0.0, 0.0),
        ])),
    )
}

/// The lanes that do not change while the harness runs: the planet, the
/// counts, the skirt, the span and the sea's own numbers. `feed::feed_tiers`
/// writes the rest of them every frame.
fn lanes_of(at: &Tiers, sheet: &crate::water::WaterExt) -> Lanes {
    Lanes {
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
    }
}

/// One of the two seas: the same sheet wearing the same shader, differing
/// in which entry point makes its vertices and which wins the depth test.
#[allow(clippy::too_many_arguments)]
fn sea_material(
    seas: &mut Assets<SeaMaterial>,
    sheet: &crate::water::WaterExt,
    lanes: Lanes,
    leaves: &Handle<ShaderStorageBuffer>,
    raised: &Handle<ShaderStorageBuffer>,
    sites: &Handle<ShaderStorageBuffer>,
    which: Which,
    bias: f32,
) -> Handle<SeaMaterial> {
    seas.add(ExtendedMaterial {
        base: StandardMaterial {
            depth_bias: bias,
            ..crate::water::sheet_base()
        },
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
            raised: raised.clone(),
            sites: sites.clone(),
            which,
        },
    })
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
