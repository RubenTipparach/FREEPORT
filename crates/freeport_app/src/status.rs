//! The STATUS line: the harness's own line of text saying where the eye
//! is, what the streamer is doing, what time it is and every key, along
//! the bottom on foot and in the air, and under the compass strip at the
//! wheel, where the HUD has the bottom corners.

use crate::clock;
use crate::drive::Thefts;
use crate::sky;
use crate::stream::Streamer;
use crate::walk::OnFoot;
use bevy::prelude::*;

/// The status line's parts.
#[derive(Resource, Default)]
pub(crate) struct Status {
    pub walker: String,
}

/// The line of text that says where the walker stands.
#[derive(Component)]
pub(crate) struct Stat;

/// The line of text along the bottom that says where the eye is.
pub(crate) fn spawn_status(commands: &mut Commands) {
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.9, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            right: Val::Px(12.0),
            bottom: Val::Px(10.0),
            ..default()
        },
        Stat,
    ));
}

pub(crate) fn show_status(
    status: Res<Status>,
    streamer: Option<Res<Streamer>>,
    walker: Option<Res<OnFoot>>,
    thefts: Res<Thefts>,
    weather: Res<sky::Weather>,
    mut text: Query<(&mut Text, &mut Node, &mut TextLayout), With<Stat>>,
) {
    let what = streamer.map(|s| s.status()).unwrap_or_default();
    let mode = if walker.is_some() { "fly" } else { "walk" };
    if let Ok((mut text, mut node, mut layout)) = text.single_mut() {
        text.0 = format!(
            "{}\n{}   |   {}   |   F {mode}, G gas, H time, T torch, Tab wire, L LOD, Esc mouse",
            status.walker,
            what,
            clock::reading(&weather)
        );
        // At the wheel the HUD has the bottom corners, so the harness's
        // own line moves up under the compass strip, centred; on foot
        // and in the air it is the line along the bottom it always was.
        let driving = thefts.driving().is_some();
        node.bottom = if driving { Val::Auto } else { Val::Px(10.0) };
        node.top = if driving {
            Val::Px(STAT_UNDER_STRIP)
        } else {
            Val::Auto
        };
        layout.justify = if driving {
            Justify::Center
        } else {
            Justify::Left
        };
    }
}

/// Where the status line stands while the HUD is up, pixels from the
/// top: under the compass strip, which is 44 px at 3.5% of the screen.
const STAT_UNDER_STRIP: f32 = 76.0;
