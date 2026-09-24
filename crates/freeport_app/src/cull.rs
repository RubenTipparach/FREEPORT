//! What a view is SPARED drawing: the GPU's own occlusion culling on the
//! camera, and the block a town's tile casts its shadow from when what
//! the camera sees is its detail.
//!
//! A city is where both bite. On a street of the port the buildings
//! either side stand in front of nearly everything else in the town, so
//! most of what the frustum hands the GPU is drawn and then painted over;
//! and the sun's cascades reach half a kilometre, so every tile in that
//! reach is drawn again for each cascade it falls in, at whatever grade
//! the CAMERA wants it, windows and all, for a shadow that is a
//! silhouette.

use crate::tuning::Tuning;
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::experimental::occlusion_culling::OcclusionCulling;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// What a tile's block wears while it stands in for the tile's detail in
/// the sun's cascades: drawn into every shadow map as the box it is, and
/// into the camera's main pass as nothing (`shadow_only.wgsl`). It has
/// no prepass, so the camera's depth never holds it.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct ShadowOnly {}

impl Material for ShadowOnly {
    fn vertex_shader() -> ShaderRef {
        "embedded://freeport_app/shadow_only.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://freeport_app/shadow_only.wgsl".into()
    }

    /// Out of the camera's depth prepass, which would otherwise stand a
    /// box in front of every recessed pane of the detail it hides in.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        true
    }
}

/// The one `ShadowOnly` every proxy block shares.
#[derive(Resource)]
pub struct Proxy(pub Handle<ShadowOnly>);

pub struct CullPlugin;

impl Plugin for CullPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shadow_only.wgsl");
        app.add_plugins(MaterialPlugin::<ShadowOnly>::default());
        let proxy = app
            .world_mut()
            .resource_mut::<Assets<ShadowOnly>>()
            .add(ShadowOnly {});
        app.insert_resource(Proxy(proxy));
    }
}

/// The first grade whose shadow is its block's. The nearest bake keeps its
/// own, because that is the grade a walker stands INSIDE, and a block
/// round a room puts the whole room in shadow whatever its windows say.
pub const PROXY_FROM: usize = 1;

/// Whether a tile drawn at `grade` casts its own shadow, or leaves it to
/// its block wearing `ShadowOnly`.
pub fn casts(grade: usize, tuning: &Tuning) -> bool {
    !tuning.shadow_proxies || grade < PROXY_FROM
}

/// Turn the GPU's two phase occlusion culling on for the camera, which
/// already carries the `DepthPrepass` it needs (the sea's transmission
/// reads it).
///
/// NOT on the sun. Bevy 0.18 takes the component on a directional light
/// too and culls each cascade against its own last shadow map, and on
/// this build and driver it culled EVERYTHING: every cascade drew nought
/// vertices, the terrain's included, on the frame the camera's own
/// culling was measured working. A shadow pass that draws nothing reads
/// as a faster frame and is a world with no shadows.
pub fn cull_views(
    mut commands: Commands,
    tuning: Res<Tuning>,
    cameras: Query<Entity, With<Camera3d>>,
) {
    if !tuning.occlusion_culling {
        info!("occlusion culling is off (render.json)");
        return;
    }
    for e in &cameras {
        commands.entity(e).insert(OcclusionCulling);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest bake casts its own shadow and every grade past it
    /// leaves it to its block, unless the proxies are turned off.
    #[test]
    fn only_the_nearest_bake_casts_its_own_shadow() {
        let mut tuning = Tuning::default();
        assert!(casts(0, &tuning));
        assert!(!casts(1, &tuning) && !casts(2, &tuning));
        tuning.shadow_proxies = false;
        assert!(casts(1, &tuning) && casts(2, &tuning));
    }

    /// The proxy is out of the camera's depth and in the sun's.
    #[test]
    fn a_proxy_casts_a_shadow_and_holds_no_depth() {
        assert!(!ShadowOnly::enable_prepass());
        assert!(ShadowOnly::enable_shadows());
    }
}
