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
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;
use freeport_core::{hex, lod};

pub type TierMaterial = ExtendedMaterial<StandardMaterial, Tier>;

/// Which tier a material draws. It is the whole of the pipeline key,
/// because the two tiers differ in exactly one thing: which entry point of
/// `tiers.wgsl` makes their vertices.
#[repr(u32)]
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Default, Reflect)]
pub enum Which {
    #[default]
    Lod,
    Hex,
}

/// What the tiers' shaders are handed. Bindings 100 to 106 are
/// `terrain::Terrain`'s to the field, because the fragment shader IS
/// `terrain.wgsl` and a bind group layout is what a shader is compiled
/// against: if the two ever drift, the pipeline fails to build rather than
/// drawing something wrong.
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
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub normal: Handle<Image>,
    #[texture(105, dimension = "2d_array")]
    #[sampler(106)]
    pub orm: Handle<Image>,
    /// x: mean radius, y: the sea's radius, z: peak to trough of the
    /// relief, w: how many relief features fit round the planet.
    #[uniform(110)]
    pub shape: Vec4,
    /// x: octaves of relief, y: the seed, z: how many pieces a leaf's edge
    /// is cut into, w: tiles along an icosahedron edge, which is the hex
    /// grid's `n`.
    #[uniform(110)]
    pub counts: UVec4,
    /// The hex anchor: the eye's own tile in its face's own plane, which
    /// is `hex::Grid::basis`'s first answer, w: how many leaves of the
    /// storage buffer are live.
    #[uniform(110)]
    pub eye: Vec4,
    /// The lattice's first step off that anchor, w: how deep a column's
    /// skirt hangs, metres.
    #[uniform(110)]
    pub lat1: Vec4,
    /// Its second step, w: how many tiles the hex window reaches.
    #[uniform(110)]
    pub lat2: Vec4,
    /// Planet-LOD's leaves, three corners a triangle, as directions.
    #[storage(111, read_only)]
    pub leaves: Handle<ShaderStorageBuffer>,
    /// Which tier: the key, and never read by a shader.
    pub which: Which,
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
            }
            .into(),
        );
        Ok(())
    }
}

pub struct TiersPlugin;

/// A handle held so `field.wgsl` is LOADED rather than merely registered.
/// A shader nothing has asked for is not in the asset system, and an
/// import that is not in the asset system is a pipeline Bevy quietly
/// retries for ever: the first cut of this drew an empty sky with not one
/// error in the log, because `tiers.wgsl` imports `freeport::field` and
/// nothing had ever loaded it.
#[derive(Resource)]
struct Field(#[allow(dead_code)] Handle<Shader>);

impl Plugin for TiersPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "field.wgsl");
        embedded_asset!(app, "tiers.wgsl");
        let field = app
            .world()
            .resource::<AssetServer>()
            .load("embedded://freeport_app/field.wgsl");
        app.insert_resource(Field(field));
        app.add_plugins(MaterialPlugin::<TierMaterial>::default());
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
/// Leaves the far tier is built for. Planet-LOD at ratio 6 picks about
/// three thousand from the ground and forty four from three radii up, so
/// this is slack rather than a limit; it is not MORE slack than that
/// because every vertex the mesh carries past the live count still runs
/// the vertex stage far enough to work out it has nothing to say.
const MOST_LEAVES: usize = 8_192;

/// Both tiers, their meshes, their materials and the buffer the far tier's
/// leaves ride in.
#[derive(Resource)]
pub struct Drawn {
    pub near: Handle<TierMaterial>,
    pub far: Handle<TierMaterial>,
    pub leaves: Handle<ShaderStorageBuffer>,
}

/// The two entities and everything they need. `frames` is the towns'
/// (there are none on a tier yet), `sea` the sea's radius.
pub fn spawn_tiers(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    materials: &mut Assets<TierMaterial>,
    buffers: &mut Assets<ShaderStorageBuffer>,
    at: &Tiers,
) {
    let maps = crate::terrain::terrain_maps(images);
    let (ground_tile, concrete_tile) = crate::terrain::tiles();
    let leaves = buffers.add(ShaderStorageBuffer::from(vec![
        Vec4::ZERO;
        at.most_leaves() * 3
    ]));
    let make = |which: Which| Tier {
        params: Vec4::new(ground_tile, concrete_tile, 0.0, at.sea as f32),
        centre: Vec4::ZERO,
        frames: [Vec4::ZERO; FRAMES * 3],
        albedo: maps[0].clone(),
        normal: maps[1].clone(),
        orm: maps[2].clone(),
        shape: Vec4::new(
            at.radius as f32,
            at.sea as f32,
            at.relief as f32,
            at.lumps as f32,
        ),
        counts: UVec4::new(at.octaves, at.seed, at.sub, at.grid().n),
        eye: Vec4::ZERO,
        lat1: Vec4::new(0.0, 0.0, 0.0, at.skirt as f32),
        lat2: Vec4::new(0.0, 0.0, 0.0, at.span as f32),
        leaves: leaves.clone(),
        which,
    };
    // Neither tier is culled by its winding: which way round a hexagon's
    // corners come out depends on the handedness of the lattice basis it
    // was stepped along, and a normal that points out is cheaper to
    // guarantee than a winding that does.
    let base = || StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        cull_mode: None,
        double_sided: true,
        ..default()
    };
    let far = materials.add(ExtendedMaterial {
        base: base(),
        extension: make(Which::Lod),
    });
    let near = materials.add(ExtendedMaterial {
        base: base(),
        extension: make(Which::Hex),
    });
    spawn_tier(
        commands,
        meshes.add(counted_mesh(at.lod_verts())),
        far.clone(),
    );
    spawn_tier(
        commands,
        meshes.add(counted_mesh(at.hex_verts())),
        near.clone(),
    );
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
    commands.insert_resource(Drawn { near, far, leaves });
}

/// Every frame: where the eye is, which tile it stands on, and which
/// leaves Planet-LOD picks from there. This is the whole of what the CPU
/// does for either tier.
pub fn feed_tiers(
    eye: Res<crate::Eye>,
    frame: Res<crate::stream::Frame>,
    at: Res<Tiers>,
    drawn: Res<Drawn>,
    mut materials: ResMut<Assets<TierMaterial>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut said: Local<usize>,
) {
    let clock = std::time::Instant::now();
    let here = eye.0 .0;
    let centre = frame.0.local(freeport_core::pos::WorldPos(DVec3::ZERO));
    let dir = here.normalize_or(DVec3::Z);
    let grid = at.grid();
    let (anchor, e1, e2) = grid.basis(grid.at(dir));
    // The far tier's hole is the near tier's disc less `OVERLAP` tiles, so
    // the two OVERLAP at the rim rather than meeting there. A tile of
    // overlap was not enough and the picture said so: at a grazing angle
    // the rim was a band of SKY, because the two tiers carry the same
    // height differently (a column's top is flat at its middle's height, a
    // leaf's is linear between its corners) and where the far tier stood
    // higher the line of sight went under it, over the ground behind and
    // out. Metres of overlap cost a few hundred triangles drawn under the
    // columns and close it for good.
    let hole = (at.disc() - OVERLAP * grid.spacing(at.radius) / at.radius).cos();
    let picked = lod::select(
        here,
        at.radius,
        &lod::Lod {
            ratio: at.ratio,
            detail: 0.0,
            cull: true,
        },
        hole,
        dir,
    );
    let live = picked.len().min(at.most_leaves());
    if let Some(buffer) = buffers.get_mut(&drawn.leaves) {
        let mut data: Vec<Vec4> = Vec::with_capacity(live * 3);
        for tri in picked.iter().take(live) {
            for corner in tri {
                data.push(corner.as_vec3().extend(0.0));
            }
        }
        data.resize(at.most_leaves() * 3, Vec4::ZERO);
        buffer.set_data(data.as_slice());
    }
    // What the CPU costs, said when it moves by a fifth: the whole of the
    // far tier's work is this one `select`, and the vertex stage makes
    // `sub * sub` triangles out of every leaf it picks.
    if live * 5 > *said * 6 || live * 6 < *said * 5 {
        info!(
            "Planet-LOD picked {live} leaves in {:.2} ms, drawn as {} triangles",
            clock.elapsed().as_secs_f64() * 1e3,
            live * (at.sub * at.sub) as usize,
        );
        *said = live;
    }
    for (handle, count) in [(&drawn.far, live), (&drawn.near, 0)] {
        if let Some(m) = materials.get_mut(handle) {
            m.extension.centre = centre.extend(0.0);
            m.extension.eye = anchor.as_vec3().extend(count as f32);
            m.extension.lat1 = e1.as_vec3().extend(at.skirt as f32);
            m.extension.lat2 = e2.as_vec3().extend(at.span as f32);
        }
    }
}

/// The two tiers' entities, once `spawn_world` has put `Tiers` in place.
pub fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TierMaterial>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    at: Res<Tiers>,
) {
    spawn_tiers(
        &mut commands,
        &mut meshes,
        &mut images,
        &mut materials,
        &mut buffers,
        &at,
    );
}
