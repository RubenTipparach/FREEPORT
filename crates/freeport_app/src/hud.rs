//! The DRIVER'S HUD: what a player at the wheel reads, in Bevy's own UI.
//!
//! `docs/mockups/driving-hud.html` is the page it was drawn on and
//! approved from, and every panel here stands where that page put it
//! after the owner's five readings: the wallet at the top left at the
//! size of a readout, the next pump in the bottom left corner, two ROUND
//! dials at the bottom right (the speed to `driver::TOP`, the fuel from
//! E to F with the RANGE under it), the game's own hour over the frame
//! counter at the top right, a compass strip across the top with the
//! car's heading under a lubber line and the first marker as a tick, and
//! G as a PROMPT that is up only while the car is stopped within
//! `PUMP_REACH` of a pump. No key legend: the owner's own reading was
//! that M and E are easy enough to find, and G is something the player
//! does to the pump rather than a control of the car's.
//!
//! Nothing on it is a number the core does not already compute. The
//! speed is `Driver::speed`, the gauge is `Tank`, a SHARE of a full one,
//! so the figure under it is range at eighty kilometres a fill, the
//! purse is `fuel::Wallet`, the next pump is `Network::nearest_pump`, and
//! the hour is `clock::reading`, the same function the status line and
//! the time menu read. The frame counter is a `std::time::Instant` and
//! never Bevy's `Time`, whose delta is clamped and lies exactly when it
//! matters, and it prints the milliseconds as well as the rate, because
//! 16.7 ms is a number that can be held against a budget and 60 fps is
//! not.
//!
//! An ARC is a ring node with a CONIC border gradient (two hard stops,
//! the colour to the share and nothing after), which is what a ring
//! swept from the bottom left clockwise to the bottom right is in Bevy's
//! UI, and a NEEDLE is a bar hung off a zero sized pivot at the dial's
//! middle turned by a `UiTransform`. Neither is a texture and neither is
//! a mesh, so the dial costs what a panel costs.

use crate::clock;
use crate::drive::Thefts;
use crate::fuel::Wallet;
use crate::map::MapView;
use crate::roads::Network;
use crate::sky::Weather;
use crate::world::Ground;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::ui::{AngularColorStop, BorderGradient, ConicGradient, Gradient, UiPosition, Val2};
use freeport_core::driver::TOP;
use freeport_core::fuel::{PUMP_REACH, STOPPED, TANK_PRICE};
use freeport_core::town;
use std::f32::consts::{PI, TAU};
use std::time::Instant;

/// The page's own palette: glass over the road, its edge, the readout
/// white, the dim grey, the amber of a needle, the route's cyan, the
/// alarm red and the green of a full tank.
///
/// The GLASS is not the page's 0.58, for the reason the map's page once
/// was not its 0.82: a browser composites in sRGB and Bevy in linear
/// light, and the same alpha over a daylit sky leaves several times the
/// light through. And the page is DARK because its world is: measured
/// off the side by side, its glass reads (32, 42, 57) over a night blue
/// scene, and this one read (103, 115, 115) at 0.82 over a noon sky of
/// (215, 249, 249), a pale grey panel where the page has smoked glass.
/// 0.94 is the page's own darkness over the brightest sky this world
/// has, and over a dark street it is the page exactly.
pub const GLASS: Color = Color::srgba_u8(12, 14, 17, 240);
pub const EDGE: Color = Color::srgba_u8(233, 230, 216, 41);
/// A dial's own face under its arc, the page's `rgba(12,14,17,0.35)`
/// raised the same way, its rim, and the track the arc fills.
const FACE: Color = Color::srgba_u8(12, 14, 17, 130);
const RIM: Color = Color::srgba_u8(233, 230, 216, 36);
const TRACK: Color = Color::srgba_u8(233, 230, 216, 56);
/// A tick on the speed dial, the page's half white.
const TICK: Color = Color::srgba_u8(233, 230, 216, 128);
pub const HUD: Color = Color::srgb_u8(233, 230, 216);
pub const DIM: Color = Color::srgb_u8(154, 152, 140);
pub const AMBER: Color = Color::srgb_u8(240, 179, 74);
pub const ROUTE: Color = Color::srgb_u8(127, 208, 232);
pub const ALARM: Color = Color::srgb_u8(224, 81, 63);
pub const OK: Color = Color::srgb_u8(140, 207, 122);

/// How far in from the screen's edge a panel stands, shares of the
/// screen: the page's own 2.2% across and 3.5% down.
const INSET_X: Val = Val::Percent(2.2);
const INSET_Y: Val = Val::Percent(3.5);
/// A dial, pixels: the page's 118 with its ring at a radius of 46, a
/// stroke of 6 and a needle of 40.
const DIAL: f32 = 118.0;
const RING: f32 = 92.0;
const STROKE: f32 = 6.0;
const NEEDLE: f32 = 40.0;
/// The face's own radius, and the speed dial's nine ticks: from the
/// page's 37 (every other one) or 41 out to 49, which is across the
/// arc's own inner half.
const FACE_R: f32 = 55.0;
const TICKS: usize = 9;
const TICK_OUT: f32 = 49.0;
const TICK_MAJOR: f32 = 37.0;
const TICK_MINOR: f32 = 41.0;
/// Where the arc starts, clockwise from the top, and how far it sweeps:
/// from the bottom left over the top to the bottom right, three quarters
/// of a turn, so a dial reads like a dial.
const ARC_FROM: f32 = 1.25 * PI;
const ARC_SWEEP: f32 = 1.5 * PI;
/// The compass strip shows this many degrees either side of the heading.
const STRIP_HALF: f32 = 60.0;
/// Under this share of a tank the gauge is amber, and under this red,
/// which is the page's own thresholds.
const LOW: f64 = 0.25;
const DRY: f64 = 0.10;
/// How often the frame counter is read, seconds: half a second, so the
/// figure can be read rather than watched flicker.
const COUNT_EVERY: f64 = 0.5;

/// The HUD's root, shown at the wheel and nowhere else.
#[derive(Component)]
pub struct Hud;

/// A line of text on it, by what it says.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Readout {
    Speed,
    Range,
    Cash,
    Pump,
    Clock,
    Fps,
    /// The first marker's distance, on the compass strip.
    Goal,
}

/// Which dial a ring or a needle belongs to.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Dial {
    Speed,
    Fuel,
}

/// The pivot a needle hangs off.
#[derive(Component)]
pub struct Needle;

/// The compass strip, and a tick or a letter on it at a bearing, degrees
/// clockwise from north.
#[derive(Component)]
pub struct Compass;
#[derive(Component)]
pub struct Bearing(pub f32);
/// The first marker's own tick on the strip.
#[derive(Component)]
pub struct GoalTick;

/// The G prompt.
#[derive(Component)]
pub struct Prompt;

/// One glass panel: the page's own translucent dark with a hairline edge.
fn glass(node: Node) -> impl Bundle {
    glass_edged(node, EDGE)
}

/// The same panel with an edge of its own colour: the prompt's is amber.
/// One function, because a bundle carrying `BorderColor` twice is a
/// panic at spawn and not a compile error.
fn glass_edged(node: Node, edge: Color) -> impl Bundle {
    (
        Node {
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..node
        },
        BackgroundColor(GLASS),
        BorderColor::all(edge),
    )
}

/// A line of text at a size and a colour.
fn text(what: &str, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(what),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(colour),
    )
}

/// A small uppercase label, the page's own `.label`.
fn label(what: &str) -> impl Bundle {
    text(what, 10.0, DIM)
}

/// The whole HUD, once, hidden.
pub fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::ZERO,
                right: Val::ZERO,
                top: Val::ZERO,
                bottom: Val::ZERO,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
            Hud,
        ))
        .with_children(|hud| {
            purse(hud);
            station(hud);
            dials(hud);
            corner(hud);
            compass(hud);
            prompt(hud);
        });
}

/// Top left: the wallet, a readout and not a headline.
fn purse(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn(glass(Node {
        position_type: PositionType::Absolute,
        left: INSET_X,
        top: INSET_Y,
        min_width: Val::Px(150.0),
        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Baseline,
        column_gap: Val::Px(14.0),
        ..default()
    }))
    .with_children(|row| {
        row.spawn(label("WALLET"));
        row.spawn((text("$1,000", 15.0, HUD), Readout::Cash));
    });
}

/// Bottom left: the next pump, which is the decision the gauge is for.
fn station(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn(glass(Node {
        position_type: PositionType::Absolute,
        left: INSET_X,
        bottom: INSET_Y,
        min_width: Val::Px(150.0),
        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Baseline,
        column_gap: Val::Px(14.0),
        ..default()
    }))
    .with_children(|row| {
        row.spawn(label("NEXT PUMP"));
        row.spawn((text("", 14.0, HUD), Readout::Pump));
    });
}

/// Bottom right: the two round dials.
fn dials(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn(glass(Node {
        position_type: PositionType::Absolute,
        right: INSET_X,
        bottom: INSET_Y,
        padding: UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(8.0), Val::Px(6.0)),
        column_gap: Val::Px(10.0),
        align_items: AlignItems::FlexEnd,
        ..default()
    }))
    .with_children(|row| {
        dial(row, Dial::Speed, "KM/H", Readout::Speed);
        dial(row, Dial::Fuel, "FUEL", Readout::Range);
    });
}

/// One dial: its label, the track, the fill, the needle on its pivot and
/// the figure under it, in that order because a later child draws over
/// an earlier one.
fn dial(row: &mut RelatedSpawnerCommands<'_, ChildOf>, which: Dial, name: &str, figure: Readout) {
    let inset = (DIAL - RING) * 0.5;
    row.spawn(Node {
        width: Val::Px(DIAL),
        height: Val::Px(DIAL),
        ..default()
    })
    .with_children(|d| {
        d.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::ZERO,
                right: Val::ZERO,
                top: Val::Px(30.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            children![label(name)],
        ));
        d.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(DIAL * 0.5 - FACE_R),
                top: Val::Px(DIAL * 0.5 - FACE_R),
                width: Val::Px(FACE_R * 2.0),
                height: Val::Px(FACE_R * 2.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(FACE),
            BorderColor::all(RIM),
        ));
        d.spawn((ring(inset), BorderGradient(vec![conic(TRACK, 1.0)])));
        d.spawn((ring(inset), BorderGradient(vec![conic(HUD, 0.0)]), which));
        let ticks: Vec<(f32, f32, f32)> = match which {
            // The speed dial's nine, every other one longer.
            Dial::Speed => (0..TICKS)
                .map(|k| {
                    let from = if k % 2 == 0 { TICK_MAJOR } else { TICK_MINOR };
                    (k as f32 / (TICKS - 1) as f32, from, TICK_OUT)
                })
                .collect(),
            // The fuel dial's one, at half a tank.
            Dial::Fuel => vec![(0.5, 42.0, 50.0)],
        };
        for (at, from, to) in ticks {
            d.spawn(spoke(ARC_FROM + ARC_SWEEP * at, from, to, 1.5, TICK));
        }
        d.spawn((spoke(ARC_FROM, 0.0, NEEDLE, 3.0, AMBER), which, Needle));
        d.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::ZERO,
                right: Val::ZERO,
                bottom: Val::Px(2.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            children![(
                text("0", 21.0, HUD),
                figure,
                children![(
                    TextSpan::new(if which == Dial::Fuel { " km" } else { "" }),
                    TextFont {
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(DIM),
                )],
            )],
        ));
        if which == Dial::Fuel {
            for (mark, left, right) in [
                ("E", Val::Px(8.0), Val::Auto),
                ("F", Val::Auto, Val::Px(8.0)),
            ] {
                d.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        right,
                        bottom: Val::Px(8.0),
                        ..default()
                    },
                    children![label(mark)],
                ));
            }
        }
    });
}

/// A ring: the dial's circle as a bordered node with nothing inside.
fn ring(inset: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(inset),
        top: Val::Px(inset),
        width: Val::Px(RING),
        height: Val::Px(RING),
        border: UiRect::all(Val::Px(STROKE)),
        border_radius: BorderRadius::MAX,
        ..default()
    }
}

/// A bar out from a dial's middle along a turn, from `from` to `to`
/// pixels out: a node of no size at the middle turned by the angle, the
/// bar hung off it. A needle is one from the middle out; a tick is one
/// from part way.
fn spoke(turn: f32, from: f32, to: f32, width: f32, colour: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(DIAL * 0.5),
            top: Val::Px(DIAL * 0.5),
            width: Val::ZERO,
            height: Val::ZERO,
            ..default()
        },
        UiTransform::from_rotation(Rot2::radians(turn)),
        children![(
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(-width * 0.5),
                bottom: Val::Px(from),
                width: Val::Px(width),
                height: Val::Px(to - from),
                border_radius: BorderRadius::all(Val::Px(width * 0.5)),
                ..default()
            },
            BackgroundColor(colour),
        )],
    )
}

/// The arc: a conic gradient from the bottom left, the colour out to
/// `share` of the sweep and nothing past it, on the ring's border.
fn conic(colour: Color, share: f32) -> Gradient {
    let to = ARC_SWEEP * share.clamp(0.0, 1.0);
    Gradient::Conic(ConicGradient {
        color_space: default(),
        start: ARC_FROM,
        position: UiPosition::CENTER,
        stops: vec![
            AngularColorStop::new(colour, 0.0),
            AngularColorStop::new(colour, to),
            AngularColorStop::new(Color::NONE, to),
            AngularColorStop::new(Color::NONE, TAU),
        ],
    })
}

/// Top right: the hour over the frame counter.
fn corner(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn(glass(Node {
        position_type: PositionType::Absolute,
        right: INSET_X,
        top: INSET_Y,
        padding: UiRect::new(Val::Px(9.0), Val::Px(9.0), Val::Px(5.0), Val::Px(4.0)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexEnd,
        row_gap: Val::Px(1.0),
        ..default()
    }))
    .with_children(|col| {
        col.spawn((text("", 17.0, HUD), Readout::Clock));
        col.spawn((text("", 12.0, DIM), Readout::Fps));
    });
}

/// Top centre: the compass strip, a window onto a band of bearings that
/// slides under a lubber line as the car turns.
fn compass(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn((
        glass(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: INSET_Y,
            width: Val::Percent(46.0),
            min_width: Val::Px(280.0),
            height: Val::Px(44.0),
            overflow: Overflow::clip(),
            ..default()
        }),
        UiTransform::from_translation(Val2 {
            x: Val::Percent(-50.0),
            y: Val::ZERO,
        }),
        Compass,
    ))
    .with_children(|strip| {
        for k in 0..24 {
            let deg = k as f32 * 15.0;
            let tall = k % 3 == 0;
            strip.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::ZERO,
                    width: Val::Px(1.0),
                    height: Val::Px(if tall { 12.0 } else { 6.0 }),
                    ..default()
                },
                BackgroundColor(if tall { HUD } else { DIM }),
                Bearing(deg),
            ));
        }
        for (k, name) in ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
            .iter()
            .enumerate()
        {
            strip.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(5.0),
                    width: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Bearing(k as f32 * 45.0),
                children![text(name, 12.0, HUD)],
            ));
        }
        strip.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::ZERO,
                bottom: Val::ZERO,
                width: Val::Px(2.0),
                ..default()
            },
            UiTransform::from_translation(Val2 {
                x: Val::Px(-1.0),
                y: Val::ZERO,
            }),
            BackgroundColor(AMBER),
        ));
        strip.spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::ZERO,
                width: Val::Px(2.0),
                height: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(ROUTE),
            Visibility::Hidden,
            GoalTick,
        ));
        strip.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                top: Val::Px(13.0),
                ..default()
            },
            children![(text("", 13.0, ROUTE), Readout::Goal)],
        ));
    });
}

/// Bottom centre: the G prompt, up only while the car stands at a pump.
fn prompt(hud: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    hud.spawn((
        glass_edged(
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                bottom: Val::Percent(14.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Baseline,
                ..default()
            },
            AMBER,
        ),
        UiTransform::from_translation(Val2 {
            x: Val::Percent(-50.0),
            y: Val::ZERO,
        }),
        Visibility::Hidden,
        Prompt,
    ))
    .with_children(|row| {
        row.spawn(text("G", 15.0, AMBER));
        row.spawn(text("fill the tank", 15.0, HUD));
        row.spawn(text(&format!("${TANK_PRICE}"), 15.0, DIM));
    });
}

/// What the HUD reads, gathered so the system that writes it is not a
/// system with nine arguments.
#[derive(SystemParam)]
pub struct Read<'w> {
    thefts: Res<'w, Thefts>,
    wallet: Res<'w, Wallet>,
    roads: Res<'w, Network>,
    weather: Res<'w, Weather>,
    map: Res<'w, MapView>,
}

/// What the HUD writes: the root's visibility, every line of text, the
/// two fills, the two needles and the prompt.
#[derive(SystemParam)]
pub struct Panels<'w, 's> {
    root: Query<'w, 's, &'static mut Visibility, With<Hud>>,
    texts: Query<'w, 's, (&'static mut Text, &'static mut TextColor, &'static Readout)>,
    rings: Query<'w, 's, (&'static mut BorderGradient, &'static Dial), Without<Needle>>,
    needles: Query<'w, 's, (&'static mut UiTransform, &'static Dial), With<Needle>>,
    prompt: Query<'w, 's, &'static mut Visibility, (With<Prompt>, Without<Hud>)>,
}

/// The frame counter's own clock: frames since it was last read, and
/// the rate and the frame time it last read.
#[derive(Default)]
pub struct Frames {
    since: Option<Instant>,
    count: u32,
    fps: f64,
    ms: f64,
}

impl Frames {
    /// One more frame, and the figures once `COUNT_EVERY` has passed.
    fn tick(&mut self) {
        let now = Instant::now();
        let Some(since) = self.since else {
            self.since = Some(now);
            return;
        };
        self.count += 1;
        let ran = (now - since).as_secs_f64();
        if ran >= COUNT_EVERY {
            self.fps = self.count as f64 / ran;
            self.ms = if self.fps > 0.0 {
                1000.0 / self.fps
            } else {
                0.0
            };
            self.count = 0;
            self.since = Some(now);
        }
    }
}

/// Every readout off the car at the wheel, and the HUD shown or not.
pub fn show_hud(read: Read, mut hud: Panels, mut frames: Local<Frames>) {
    frames.tick();
    let driving = read.thefts.driving();
    let show = driving.is_some() && !read.map.open;
    for mut seen in &mut hud.root {
        *seen = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let Some(theft) = driving else { return };
    let car = &theft.car;
    let pace = (car.speed.abs() / TOP).clamp(0.0, 1.0) as f32;
    let share = car.tank.0.clamp(0.0, 1.0);
    let fuel = if share < DRY {
        ALARM
    } else if share < LOW {
        AMBER
    } else {
        OK
    };
    let pump = read.roads.nearest_pump(car.dir * car.foot);
    for (mut line, mut paint, what) in &mut hud.texts {
        match what {
            Readout::Speed => line.0 = format!("{:.0}", car.speed.abs() * 3.6),
            Readout::Range => line.0 = format!("{:.0}", car.tank.reach() / 1000.0),
            Readout::Cash => line.0 = dollars(read.wallet.0.dollars),
            Readout::Pump => {
                line.0 = pump.map_or("none".to_string(), |(d, _)| format!("{:.1} km", d / 1000.0));
                paint.0 = if pump.is_some_and(|(d, _)| d > car.tank.reach()) {
                    ALARM
                } else {
                    HUD
                };
            }
            Readout::Clock => line.0 = clock::reading(&read.weather),
            Readout::Fps => line.0 = format!("{:.0} fps  {:.1} ms", frames.fps, frames.ms),
            Readout::Goal => {}
        }
    }
    for (mut fill, which) in &mut hud.rings {
        *fill = match which {
            Dial::Speed => BorderGradient(vec![conic(HUD, pace)]),
            Dial::Fuel => BorderGradient(vec![conic(fuel, share as f32)]),
        };
    }
    for (mut turn, which) in &mut hud.needles {
        let at = match which {
            Dial::Speed => pace,
            Dial::Fuel => share as f32,
        };
        turn.rotation = Rot2::radians(ARC_FROM + ARC_SWEEP * at);
    }
    let standing = at_pump(car.speed, pump.map(|(d, _)| d));
    for mut seen in &mut hud.prompt {
        *seen = if standing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Whether the G prompt is up: the car stopped within a forecourt's reach,
/// which is `fuel`'s own rule for the key and never a second one here.
fn at_pump(speed: f64, pump: Option<f64>) -> bool {
    speed.abs() < STOPPED && pump.is_some_and(|d| d <= PUMP_REACH)
}

/// A purse with its thousands marked, which is how a sum of money reads.
pub fn dollars(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + 4);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("${out}")
}

/// A heading's bearing, degrees clockwise from north at a place on the
/// sphere: nought is north, ninety east.
pub fn bearing(dir: DVec3, fwd: DVec3) -> f64 {
    let (east, north) = town::frame_at(dir);
    fwd.dot(east)
        .atan2(fwd.dot(north))
        .to_degrees()
        .rem_euclid(360.0)
}

/// The bearing from a place to another, along the great circle, and how
/// far it is over the ground.
pub fn toward(dir: DVec3, to: DVec3, radius: f64) -> (f64, f64) {
    let flat = to - dir * to.dot(dir);
    (bearing(dir, flat), dir.angle_between(to) * radius)
}

/// The signed turn from one bearing to another, degrees, in the half
/// turn either way.
fn across(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// The goal's tick on the strip, which is placed like a bearing's and is
/// not one.
type GoalTicks<'w, 's> =
    Query<'w, 's, (&'static mut Node, &'static mut Visibility), (With<GoalTick>, Without<Bearing>)>;

/// The strip's ticks and letters slid under the lubber line, and the
/// first marker's tick and distance on it.
pub fn turn_compass(
    thefts: Res<Thefts>,
    route: crate::route::Planned,
    ground: Res<Ground>,
    strip: Query<&ComputedNode, With<Compass>>,
    mut marks: Query<(&mut Node, &mut Visibility, &Bearing)>,
    mut goal: GoalTicks,
    mut said: Query<(&mut Text, &Readout)>,
) {
    let Some(theft) = thefts.driving() else {
        return;
    };
    let Ok(strip) = strip.single() else { return };
    let width = strip.size().x * strip.inverse_scale_factor;
    if width <= 0.0 {
        return;
    }
    let per_degree = width / (2.0 * STRIP_HALF);
    let heading = bearing(theft.car.dir, theft.car.fwd);
    let place = |deg: f64, own: f32| -> Option<f32> {
        let off = across(heading, deg) as f32;
        (off.abs() <= STRIP_HALF + 8.0).then_some(width * 0.5 + off * per_degree - own * 0.5)
    };
    for (mut node, mut seen, at) in &mut marks {
        let own = if let Val::Px(w) = node.width { w } else { 0.0 };
        match place(at.0 as f64, own) {
            Some(x) => {
                node.left = Val::Px(x);
                *seen = Visibility::Inherited;
            }
            None => *seen = Visibility::Hidden,
        }
    }
    // The tick is where the marker IS and the figure how far it is to
    // DRIVE there, along the roads the map planned: the bearing is what
    // says which way, and the distance is what the tank is held against.
    let first = route.markers.0.first().map(|&m| {
        let (deg, crow) = toward(theft.car.dir, m, ground.0.planet.radius);
        (deg, route.first().map_or(crow, |l| l.metres))
    });
    for (mut node, mut seen) in &mut goal {
        match first.and_then(|(deg, _)| place(deg, 2.0)) {
            Some(x) => {
                node.left = Val::Px(x);
                *seen = Visibility::Inherited;
            }
            None => *seen = Visibility::Hidden,
        }
    }
    for (mut line, what) in &mut said {
        if *what == Readout::Goal {
            line.0 = first.map_or(String::new(), |(_, m)| format!("{:.1} km", m / 1000.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heading's bearing is read off the place's own east and north:
    /// north is nought, east ninety, and the sign is the compass's.
    #[test]
    fn a_bearing_is_clockwise_from_north_at_the_place() {
        let dir = DVec3::new(0.3, 0.5, 0.81).normalize();
        let (east, north) = town::frame_at(dir);
        assert!((bearing(dir, north)).abs() < 1e-9);
        assert!((bearing(dir, east) - 90.0).abs() < 1e-9);
        assert!((bearing(dir, -north) - 180.0).abs() < 1e-9);
        assert!((bearing(dir, -east) - 270.0).abs() < 1e-9);
        // A heading with some UP in it is squared to the tangent plane
        // by the frame's own dot products, so a car on a hill reads the
        // same bearing as one on the flat.
        let tilted = (north + dir * 0.4).normalize();
        assert!((bearing(dir, tilted)).abs() < 1e-9);
    }

    /// The bearing TOWARD a place is the great circle's initial bearing,
    /// and the distance is along the ground: a marker due east a
    /// kilometre away reads ninety and a thousand metres.
    #[test]
    fn a_marker_is_a_bearing_and_a_distance_over_the_ground() {
        let radius = 1_000_000.0;
        let dir = DVec3::new(0.3, 0.5, 0.81).normalize();
        let (east, _) = town::frame_at(dir);
        let to = (dir * radius + east * 1000.0).normalize();
        let (deg, m) = toward(dir, to, radius);
        println!("bearing {deg:.4} at {m:.3} m");
        assert!((deg - 90.0).abs() < 1e-3, "{deg}");
        assert!((m - 1000.0).abs() < 0.01, "{m}");
        assert!((across(350.0, 10.0) - 20.0).abs() < 1e-9);
        assert!((across(10.0, 350.0) + 20.0).abs() < 1e-9);
    }

    /// The prompt is up exactly when the car stands at a pump: stopped,
    /// and within the forecourt's own reach.
    #[test]
    fn the_prompt_is_up_only_stopped_at_a_pump() {
        assert!(at_pump(0.0, Some(PUMP_REACH)));
        assert!(at_pump(-STOPPED * 0.5, Some(3.0)));
        assert!(!at_pump(STOPPED, Some(3.0)), "rolling");
        assert!(!at_pump(0.0, Some(PUMP_REACH + 0.1)), "a stride too far");
        assert!(!at_pump(0.0, None), "no pump on the body");
    }

    /// A purse reads with its thousands marked.
    #[test]
    fn a_purse_reads_with_its_thousands_marked() {
        assert_eq!(dollars(0), "$0");
        assert_eq!(dollars(990), "$990");
        assert_eq!(dollars(1000), "$1,000");
        assert_eq!(dollars(1_234_567), "$1,234,567");
    }

    /// The frame counter reads nothing until half a second has run, and
    /// then a rate off its own clock.
    #[test]
    fn the_frame_counter_reads_off_its_own_clock() {
        let mut f = Frames::default();
        f.tick();
        assert_eq!(f.fps, 0.0);
        // Sixty frames inside the window, and then the window is a second
        // old: the next tick is the sixty first and closes it.
        for _ in 0..60 {
            f.tick();
        }
        assert_eq!(f.fps, 0.0, "the window has not run out yet");
        f.since = Some(Instant::now() - std::time::Duration::from_secs_f64(1.0));
        f.tick();
        println!("{:.1} fps, {:.2} ms", f.fps, f.ms);
        assert!(f.fps > 60.0 && f.fps <= 61.5, "{}", f.fps);
        assert!((f.ms - 1000.0 / f.fps).abs() < 1e-9);
        assert_eq!(f.count, 0, "the window starts again");
    }
}
