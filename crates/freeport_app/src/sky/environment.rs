//! Cache Bevy's filtered sky light until its CPU-baked source image changes.

use bevy::core_pipeline::core_3d::graph::Core3d;
use bevy::pbr::generate::{
    prepare_generated_environment_map_intermediate_textures, GeneratorBindGroups,
    GeneratorPipelines, IntermediateTextures, RenderEnvironmentMap,
};
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_graph::RenderSubGraph;
use bevy::render::render_resource::{PipelineCache, TextureId};
use bevy::render::renderer::RenderQueue;
use bevy::render::view::ViewTarget;
use bevy::render::{Render, RenderApp, RenderSystems};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Only for CPU-baked image assets: replacing their GPU textures regenerates
/// the filtered light. Animated cubemaps written in place on the GPU must keep
/// Bevy's normal realtime generation and must not have this marker.
#[derive(Component, Clone, Copy, ExtractComponent)]
pub(crate) struct StaticEnvironment;

pub(super) struct StaticEnvironmentPlugin;

impl Plugin for StaticEnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<StaticEnvironment>::default());
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render.add_systems(
            Render,
            suppress_cached
                .in_set(RenderSystems::PrepareResources)
                .before(prepare_generated_environment_map_intermediate_textures),
        );
        render.add_systems(
            Render,
            prepare_submission
                .after(RenderSystems::Prepare)
                .before(RenderSystems::Render),
        );
        render.add_systems(Render, mark_submitted.in_set(RenderSystems::Cleanup));
    }
}

#[derive(Component)]
struct FilteredSky {
    textures: [TextureId; 3],
    completed: Option<Arc<AtomicBool>>,
    ready: bool,
}

impl FilteredSky {
    fn new(textures: [TextureId; 3]) -> Self {
        Self {
            textures,
            completed: None,
            ready: false,
        }
    }

    fn matches_complete(&mut self, textures: [TextureId; 3]) -> bool {
        self.ready = false;
        if self.textures != textures {
            *self = Self::new(textures);
        }
        self.completed
            .as_ref()
            .is_some_and(|done| done.load(Ordering::Acquire))
    }

    fn submit(&mut self) -> Option<Arc<AtomicBool>> {
        if !std::mem::take(&mut self.ready) || self.completed.is_some() {
            return None;
        }
        let done = Arc::new(AtomicBool::new(false));
        self.completed = Some(done.clone());
        Some(done)
    }
}

fn suppress_cached(
    mut commands: Commands,
    mut skies: Query<
        (Entity, &RenderEnvironmentMap, Option<&mut FilteredSky>),
        With<StaticEnvironment>,
    >,
) {
    for (entity, sky, cached) in &mut skies {
        let textures = [
            sky.environment_map.texture.id(),
            sky.diffuse_map.texture.id(),
            sky.specular_map.texture.id(),
        ];
        let Some(mut cached) = cached else {
            commands.entity(entity).insert(FilteredSky::new(textures));
            continue;
        };
        if cached.textures != textures {
            // A prepare dependency may still be loading. Old bindings must
            // not certify a bake for replacement textures while Bevy retries.
            commands
                .entity(entity)
                .remove::<(GeneratorBindGroups, IntermediateTextures)>();
        }
        if cached.matches_complete(textures) {
            // Extraction reinserts the source each frame. Suppress just the
            // generator before its prepare systems, preserving EnvironmentMapLight
            // and the populated diffuse/specular textures used by PBR.
            commands.entity(entity).remove::<(
                RenderEnvironmentMap,
                GeneratorBindGroups,
                IntermediateTextures,
            )>();
        }
    }
}

type GeneratingSky = (
    With<StaticEnvironment>,
    With<RenderEnvironmentMap>,
    With<GeneratorBindGroups>,
);

fn prepare_submission(
    pipelines: Option<Res<GeneratorPipelines>>,
    cache: Res<PipelineCache>,
    views: Query<(&ExtractedCamera, &ViewTarget)>,
    mut skies: Query<&mut FilteredSky, GeneratingSky>,
) {
    let Some(pipelines) = pipelines else {
        return;
    };
    let rendered = views.iter().any(|(camera, _)| {
        camera.render_graph == Core3d.intern()
            && camera
                .physical_viewport_size
                .is_some_and(|s| s.x > 0 && s.y > 0)
    });
    let ready = [
        pipelines.copy,
        pipelines.downsample_first,
        pipelines.downsample_second,
        pipelines.radiance,
        pipelines.irradiance,
    ]
    .iter()
    .all(|id| cache.get_compute_pipeline(*id).is_some());
    if !rendered || !ready {
        return;
    }
    // Snapshot before graph execution: a pipeline becoming ready after its
    // node skipped must not let Cleanup certify an uninitialized output.
    for mut sky in &mut skies {
        sky.ready = true;
    }
}

fn mark_submitted(
    queue: Res<RenderQueue>,
    mut skies: Query<&mut FilteredSky, With<StaticEnvironment>>,
) {
    // Cleanup follows graph execution and queue submission. An unavailable
    // pipeline/view never arms the bake. No thread waits for this callback.
    for mut sky in &mut skies {
        if let Some(done) = sky.submit() {
            queue.on_submitted_work_done(move || done.store(true, Ordering::Release));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_is_reused_only_after_its_submission_completes() {
        let textures = std::array::from_fn(|_| TextureId::new());
        let mut sky = FilteredSky::new(textures);
        assert!(!sky.matches_complete(textures));
        assert!(sky.submit().is_none());
        sky.ready = true;
        let completion = sky.submit().unwrap();
        assert!(!sky.matches_complete(textures));
        assert!(sky.submit().is_none());
        completion.store(true, Ordering::Release);
        assert!(sky.matches_complete(textures));
        assert!(sky.submit().is_none());
    }

    #[test]
    fn replacing_any_texture_regenerates_and_ignores_an_old_completion() {
        for changed in 0..3 {
            let mut textures = std::array::from_fn(|_| TextureId::new());
            let mut sky = FilteredSky::new(textures);
            sky.ready = true;
            let previous = sky.submit().unwrap();
            textures[changed] = TextureId::new();
            assert!(!sky.matches_complete(textures));
            previous.store(true, Ordering::Release);
            assert!(!sky.matches_complete(textures));
            assert!(sky.submit().is_none());
            sky.ready = true;
            let replacement = sky.submit().unwrap();
            replacement.store(true, Ordering::Release);
            assert!(sky.matches_complete(textures));
        }
    }
}
