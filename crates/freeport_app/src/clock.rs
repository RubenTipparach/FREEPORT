//! What o'clock it is, and the one panel a player can change it from.
//!
//! The TIME is `sky::Weather`'s (`now`, a clock that only grows, off an
//! offset `start` the harness sets from `--hour`), and everything that
//! reads the sun reads that one number. This file is the other side of
//! it: the readout, and a drawn panel with the hour on it, because a
//! world whose day is four hours long is a world where "come back at
//! dusk" is a thing a player should be able to ask for rather than wait
//! forty minutes for.
//!
//! It writes `Weather::start` and NOTHING else, which is what keeps the
//! flag and the menu meaning the same time: `--hour 6` and pressing DAWN
//! both end up at `day::at_oclock`, one function, and the status line
//! reads the answer back through `day::oclock` either way.

use crate::sky::Weather;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::day;

/// Whether the panel is open. A resource rather than a query on the
/// node, because `grab_mouse` has to know: a click on a button must not
/// also be a click that takes the mouse back off the player.
#[derive(Resource, Default)]
pub struct TimeMenu {
    pub open: bool,
}

/// The panel itself, and the line of text on it.
#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct Readout;

/// What pressing a button DOES. A component rather than an index into a
/// table somewhere: the button and what it means are one thing, which is
/// this project's own rule about a marker being the interface.
#[derive(Component, Clone, Copy)]
pub enum Press {
    /// Move the clock by a share of a whole day.
    By(f64),
    /// Set it to an hour of the twenty four hour dial.
    At(f64),
    /// Hold the sun where it is, or let it go again.
    Freeze,
}

/// The key that opens it.
///
/// H for the HOUR, because T is the TORCH (`lamps::hold_torch`) and two
/// features on one key is one press doing two things: the first cut of
/// this panel took T and a player opening the clock would have turned
/// his torch on and off every time. Every key in this harness is in one
/// table, which is the status line `main.rs` prints, and that is where a
/// collision is visible.
pub const KEY: KeyCode = KeyCode::KeyH;

/// How the panel is painted: the deck's own dark, a lighter button and a
/// lighter one again under the pointer, so a press has somewhere to go.
const PANEL: Color = Color::srgba(0.05, 0.06, 0.08, 0.86);
const BUTTON: Color = Color::srgb(0.16, 0.18, 0.22);
const HOVER: Color = Color::srgb(0.26, 0.29, 0.34);
const HELD: Color = Color::srgb(0.38, 0.42, 0.48);

/// The panel, once, hidden: a title, the hour, a row that STEPS the
/// clock, a row of the four hours worth naming, and the freeze.
pub fn spawn_menu(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(14.0),
                width: Val::Px(268.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(PANEL),
            Visibility::Hidden,
            Panel,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("TIME OF DAY"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.62, 0.66, 0.72)),
            ));
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.93, 0.86)),
                Readout,
            ));
            // A day is `day::DAY` long, so an hour of the dial is a
            // twenty fourth of it and ten minutes a hundred and
            // forty fourth: the steps are SHARES of a day rather than
            // seconds, so a body with a longer one steps in its own
            // hours and not in somebody else's.
            row(
                panel,
                &[
                    ("-1h", Press::By(-1.0 / 24.0)),
                    ("-10m", Press::By(-1.0 / 144.0)),
                    ("+10m", Press::By(1.0 / 144.0)),
                    ("+1h", Press::By(1.0 / 24.0)),
                ],
            );
            row(
                panel,
                &[
                    ("Dawn", Press::At(6.0)),
                    ("Noon", Press::At(12.0)),
                    ("Dusk", Press::At(18.0)),
                    ("Night", Press::At(0.0)),
                ],
            );
            row(panel, &[("Freeze the sun", Press::Freeze)]);
        });
}

/// One row of buttons across the panel.
fn row(panel: &mut RelatedSpawnerCommands<'_, ChildOf>, keys: &[(&str, Press)]) {
    panel
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|line| {
            for (label, press) in keys {
                line.spawn((
                    Button,
                    Node {
                        flex_grow: 1.0,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::axes(Val::Px(4.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                    *press,
                ))
                .with_child((
                    Text::new(*label),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.9, 0.88)),
                ));
            }
        });
}

/// The key, and the mouse that comes with it: a panel a player has to
/// press is a panel the cursor has to be free for, so opening it gives
/// the mouse back and closing it leaves the window to take it again.
pub fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<TimeMenu>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut panel: Query<&mut Visibility, With<Panel>>,
) {
    if !keys.just_pressed(KEY) {
        return;
    }
    menu.open = !menu.open;
    for mut show in &mut panel {
        *show = if menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if menu.open {
        if let Ok(mut cursor) = cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

/// What a press does to the clock.
///
/// Every one of them writes `Weather::start`, which is the offset
/// `--hour` sets and the only thing in the world that says what time it
/// is: the sun, the light, the dome, the fog, the sea, the lamps and the
/// bodies drawn from far off all read `Weather::sun`, which `turn_sun`
/// derives from it once a frame.
pub fn press_menu(
    time: Res<Time>,
    mut weather: ResMut<Weather>,
    mut buttons: Query<(&Interaction, &Press, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (state, press, mut paint) in &mut buttons {
        *paint = BackgroundColor(match state {
            Interaction::Pressed => HELD,
            Interaction::Hovered => HOVER,
            Interaction::None => BUTTON,
        });
        if *state != Interaction::Pressed {
            continue;
        }
        match press {
            Press::By(share) => weather.start += share * weather.day,
            Press::At(hour) => {
                // The hour is solved back into the clock at the EYE's
                // own place, which is what makes twelve noon THERE
                // rather than over some world axis: `day::oclock` reads
                // it back the same way and the two are one function.
                let want = day::at_oclock(weather.noon, weather.here, *hour, weather.day);
                weather.start = want - time.elapsed_secs_f64();
            }
            Press::Freeze => weather.frozen = !weather.frozen,
        }
    }
}

/// The hour on the panel, and whether the sun is held.
pub fn show_menu(
    menu: Res<TimeMenu>,
    weather: Res<Weather>,
    mut text: Query<&mut Text, With<Readout>>,
) {
    if !menu.open {
        return;
    }
    for mut line in &mut text {
        line.0 = format!(
            "{}{}",
            reading(&weather),
            if weather.frozen { "  (held)" } else { "" }
        );
    }
}

/// What o'clock it is where the eye stands, on the twenty four hour dial
/// `--hour` is asked in, so the flag, the menu and the status line
/// cannot mean three different times. The ONE place a time is turned
/// into words.
pub fn reading(weather: &Weather) -> String {
    let h = weather.oclock();
    format!(
        "{:02}:{:02}",
        h.floor() as u32 % 24,
        ((h.fract() * 60.0) as u32).min(59)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;

    fn weather(hour: f64) -> Weather {
        let noon = DVec3::new(0.9, 0.2, 0.39).normalize();
        // NOT the pole, which is where a clock has no hours in it at
        // all: the sun turns about `day::AXIS`, which is plus y, so its
        // elevation over a place ON that axis never changes and every
        // hour there reads as noon. The first cut of this fixture stood
        // at `DVec3::Y` and every assertion in it came back twelve.
        let here = DVec3::new(0.3, 0.5, 0.81).normalize();
        let start = day::at_oclock(noon, here, hour, day::DAY);
        Weather {
            air: freeport_core::atmos::Air::round(1_000_000.0, 8_000.0),
            sea: 1_000_000.0,
            sun: day::sun_at(noon, start, day::DAY),
            noon,
            start,
            now: start,
            day: day::DAY,
            here,
            frozen: false,
        }
    }

    /// THE MENU AND THE FLAG MEAN THE SAME TIME. Both write
    /// `Weather::start` through `day::at_oclock` and both are read back
    /// through `day::oclock`, so a panel that set a fourth hour would be
    /// a clock nobody could check against the harness.
    #[test]
    fn the_hour_the_menu_sets_is_the_hour_the_readout_says() {
        for hour in [0.0, 6.0, 11.5, 12.0, 18.0, 23.0] {
            let mut w = weather(hour);
            w.sun = day::sun_at(w.noon, w.now, w.day);
            let said = w.oclock();
            assert!(
                (said - hour).abs() < 0.02 || (said - hour).abs() > 23.98,
                "{hour} o'clock reads back as {said}"
            );
            println!("{hour:>5} o'clock reads {}", reading(&w));
        }
    }

    /// AND A STEP IS AN HOUR OF THIS BODY'S OWN DAY. The buttons move
    /// the clock by a SHARE of a day rather than by a number of seconds,
    /// so a body whose day is longer steps in its own hours.
    #[test]
    fn a_step_is_an_hour_of_the_bodys_own_day() {
        let mut w = weather(12.0);
        let before = w.oclock();
        w.start += w.day / 24.0;
        w.now = w.start;
        w.sun = day::sun_at(w.noon, w.now, w.day);
        let after = w.oclock();
        let moved = (after - before).rem_euclid(24.0);
        println!("a step took {before:.2} to {after:.2}");
        assert!(
            (moved - 1.0).abs() < 0.02,
            "an hour's button moved the clock {moved:.3} hours"
        );
    }
}
