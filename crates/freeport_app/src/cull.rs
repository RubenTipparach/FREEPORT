//! What a view is SPARED drawing: the GPU's own occlusion culling on the
//! camera, and the layer a town's tile casts its shadow from when what
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
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::experimental::occlusion_culling::OcclusionCulling;

/// The render layer only the SUN sees: a tile's solid block stands on it
/// while the camera is shown the tile's detail, so the shadow is cast
/// from a box to the eaves rather than from every window reveal. Layer 1
/// is the LOD wireframe's and 2 the map's.
pub const SHADOW_ONLY: usize = 3;

/// The first grade whose shadow is its block's. The nearest bake keeps its
/// own, because that is the grade a walker stands INSIDE, and a block
/// round a room puts the whole room in shadow whatever its windows say.
pub const PROXY_FROM: usize = 1;

/// Every layer the sun casts from: the world's, the wireframe's (so the
/// LOD view keeps its shadows) and the blocks standing in for detail.
pub fn sun_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, 1, SHADOW_ONLY])
}

/// Whether a tile drawn at `grade` casts its own shadow, or leaves it to
/// its block on `SHADOW_ONLY`.
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

    /// The sun sees the world, the wireframe and the shadow layer; the
    /// camera, on the default layer, sees none of the shadow layer.
    #[test]
    fn the_sun_sees_the_shadow_layer_and_the_camera_does_not() {
        let shadow = RenderLayers::layer(SHADOW_ONLY);
        let sun = sun_layers();
        assert!(sun.intersects(&shadow));
        assert!(sun.intersects(&RenderLayers::default()));
        assert!(sun.intersects(&RenderLayers::layer(1)));
        assert!(!RenderLayers::default().intersects(&shadow));
        assert!(!RenderLayers::layer(1).intersects(&shadow));
    }
}
