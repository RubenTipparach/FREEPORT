//! Inspect the actual terrain triangles, colored by their owning chunk's LOD.

use crate::Args;
use bevy::camera::visibility::RenderLayers;
use bevy::pbr::wireframe::{Wireframe, WireframeColor, WireframeConfig};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct LodDebug {
    pub enabled: bool,
    pub frozen: bool,
}

#[derive(Component)]
pub(crate) struct TerrainLod(pub u8);

#[derive(Component)]
pub(crate) struct Legend;

#[derive(Component)]
pub(crate) struct LegendTitle;

pub(crate) fn color(level: u8) -> Color {
    let rgb = [
        [0.1, 1.0, 1.0],
        [1.0, 0.4, 0.05],
        [0.4, 1.0, 0.1],
        [0.9, 0.25, 1.0],
        [1.0, 0.95, 0.1],
        [0.2, 0.5, 1.0],
        [1.0, 0.2, 0.5],
        [0.4, 1.0, 0.7],
        [1.0, 0.7, 0.4],
        [0.7, 0.6, 1.0],
        [1.0, 1.0, 1.0],
        [0.9, 0.7, 0.1],
        [0.1, 0.7, 0.7],
        [0.8, 0.4, 0.4],
        [0.6, 0.7, 0.4],
        [0.7, 0.4, 0.7],
    ][level.min(15) as usize];
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

pub(crate) fn spawn_legend(
    mut commands: Commands,
    args: Res<Args>,
    tuning: Res<crate::tuning::Tuning>,
) {
    let fine = args.cell_size.unwrap_or(tuning.terrain_cell_size);
    commands
        .spawn((
            Legend,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.02, 0.025, 0.88)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("LOD wireframe | L exit | K freeze rings"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                LegendTitle,
            ));
            for level in 0..args.levels {
                parent.spawn((
                    Text::new(format!(
                        "LOD {level:2}   {:7.2} m cells",
                        fine * (1u64 << level) as f64
                    )),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(color(level)),
                ));
            }
            parent.spawn((
                Text::new("Terrain only; colors include each chunk's seam triangles."),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
            ));
        });
}

pub(crate) fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<LodDebug>,
    mut wire: ResMut<WireframeConfig>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        wire.global = !wire.global;
        mode.enabled = false;
        mode.frozen = false;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        mode.enabled = !mode.enabled;
        mode.frozen = false;
        wire.global = false;
    }
    if mode.enabled && keys.just_pressed(KeyCode::KeyK) {
        mode.frozen = !mode.frozen;
    }
}

pub(crate) fn apply(
    mut commands: Commands,
    mode: Res<LodDebug>,
    chunks: Query<(Entity, Ref<TerrainLod>)>,
    cameras: Query<(Entity, Ref<Camera3d>)>,
    mut legend: Query<&mut Visibility, With<Legend>>,
    mut title: Query<&mut Text, With<LegendTitle>>,
) {
    for (entity, level) in &chunks {
        if !mode.is_changed() && !level.is_added() {
            continue;
        }
        commands
            .entity(entity)
            .insert(RenderLayers::from_layers(&[0, 1]));
        if mode.enabled {
            commands.entity(entity).insert((
                Wireframe,
                WireframeColor {
                    color: color(level.0),
                },
            ));
        } else {
            commands
                .entity(entity)
                .remove::<(Wireframe, WireframeColor)>();
        }
    }
    for (entity, camera) in &cameras {
        if mode.is_changed() || camera.is_added() {
            commands.entity(entity).insert(if mode.enabled {
                RenderLayers::layer(1)
            } else {
                RenderLayers::default()
            });
        }
    }
    if mode.is_changed() {
        for mut visibility in &mut legend {
            *visibility = if mode.enabled {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        for mut text in &mut title {
            text.0 = format!(
                "LOD wireframe | L exit | K {} rings",
                if mode.frozen {
                    "unfreeze (FROZEN)"
                } else {
                    "freeze"
                }
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_mode_colors_new_chunks_without_revealing_staged_meshes_and_exits_cleanly() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<WireframeConfig>()
            .insert_resource(LodDebug {
                enabled: true,
                frozen: false,
            })
            .add_systems(Update, (controls, apply).chain());
        let staged = app
            .world_mut()
            .spawn((TerrainLod(0), Visibility::Hidden))
            .id();
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        assert!(app.world().get::<Wireframe>(staged).is_some());
        assert_eq!(
            app.world().get::<RenderLayers>(camera),
            Some(&RenderLayers::layer(1))
        );
        assert_eq!(
            app.world().get::<Visibility>(staged),
            Some(&Visibility::Hidden)
        );
        let new = app.world_mut().spawn(TerrainLod(3)).id();
        app.update();
        assert_eq!(
            app.world().get::<WireframeColor>(new).unwrap().color,
            color(3)
        );
        assert_ne!(color(0), color(3));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyK);
        app.update();
        assert!(app.world().resource::<LodDebug>().frozen);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        assert!(app.world().resource::<WireframeConfig>().global);
        assert!(!app.world().resource::<LodDebug>().frozen);
        assert!(!app.world().resource::<LodDebug>().enabled);
        assert!(app.world().get::<WireframeColor>(new).is_none());
        assert_eq!(
            app.world().get::<RenderLayers>(camera),
            Some(&RenderLayers::default())
        );
        assert!(app.world().get::<Wireframe>(staged).is_none());
        assert_eq!(
            app.world().get::<Visibility>(staged),
            Some(&Visibility::Hidden)
        );
    }
}
