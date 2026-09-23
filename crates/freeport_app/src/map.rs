//! The MAP behind M: a plan of the roads, the settlements and the pumps
//! round the car, with markers a player sets by hand for a route.
//!
//! It is a MODE and not a screen, which is swarm-demo's sensors manager
//! rule: M dims the drive rather than leaving it, the car goes on being
//! driven under it, and Esc or M again brings the road back. What it
//! draws is the same road network the car drives, `World::routes` off
//! the atlas, and not a picture of the chart, so a road on the map is a
//! road under the wheels. A click sets a numbered marker, a right click
//! takes the last one back, the legs between them are straight lines
//! summed against the tank in the route panel, the wheel zooms about
//! the cursor from the region down to a town, a drag pans, and it opens
//! centred on the car. A marker is a PLACE and never a road: the total
//! is the crow's distance and understates a drive round a bay, and a
//! route that follows the roads between markers is A* over the road
//! graph, whose input this is.
//!
//! This file is the MODE: the chart, the view, the markers, the keys and
//! the mouse, the route panel's figures and the tests. `map/draw.rs` is
//! what is DRAWN: an OVERLAY camera of its own, a `Camera2d` a layer
//! above the world that clears nothing and draws over it, with the
//! roads, the legs, the markers and the car as gizmo lines in screen
//! pixels, the labels as `Text2d`, the sea as a mesh of wet cells
//! sampled off the planet's own field, and the panels as UI nodes aimed
//! at that camera. The projection is GNOMONIC about the map's centre,
//! the sphere seen from its own middle, which is exact both ways and is
//! what lets a click be turned back into a direction on the sphere
//! without a search.

mod draw;

use crate::args::Args;
use crate::drive::Thefts;
use crate::walk::OnFoot;
use crate::world::Ground;
use crate::Eye;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
pub use draw::{draw_map, sea_layer, spawn_map};
use freeport_core::town;

/// How much ground the map shows across the window when it opens and at
/// its widest, metres: the page's own region, a morning's drive.
const REGION: f64 = 62_000.0;
/// How far in the wheel goes past that, a factor: down to a town.
const ZOOM: f64 = 24.0;
/// One notch of the wheel, a factor on the scale.
const NOTCH: f64 = 1.15;
/// How far a press may travel and still be a click, pixels.
const CLICK: f32 = 4.0;
/// How many markers a route may carry.
const MOST: usize = 12;

/// The map's own gizmos, on the overlay's layer.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct MapGizmos;

/// The overlay camera, the dimmer under everything, the sea layer, and
/// the panels: what opening and closing the map shows and hides.
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Dim;
#[derive(Component)]
pub struct SeaLayer(Handle<Mesh>);
#[derive(Component)]
pub struct MapUi;
/// A panel a click on is a click on the panel and not on the map.
#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct ScaleBar;
#[derive(Component)]
pub struct Clear;
/// A line of text on the route panel or the scale bar.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Says {
    Legs,
    Total,
    Holds,
    Scale,
}

/// The markers a player has set, in the order set, as directions on the
/// sphere. A resource because the compass strip reads the first one.
#[derive(Resource, Default)]
pub struct Markers(pub Vec<DVec3>);

/// The sphere seen from over the map's own middle: the gnomonic
/// projection about `centre`, east across and north up, at `scale`
/// metres a pixel.
#[derive(Clone, Copy, Debug)]
pub struct Chart {
    pub centre: DVec3,
    east: DVec3,
    north: DVec3,
    pub radius: f64,
    pub scale: f64,
}

impl Chart {
    /// The chart about a direction at a scale.
    pub fn new(centre: DVec3, radius: f64, scale: f64) -> Chart {
        let (east, north) = town::frame_at(centre);
        Chart {
            centre,
            east,
            north,
            radius,
            scale,
        }
    }

    /// Where a direction lands, pixels from the middle with north up, or
    /// nothing for a direction past the horizon of the projection.
    pub fn to_px(self, dir: DVec3) -> Option<Vec2> {
        let c = dir.dot(self.centre);
        if c < 0.2 {
            return None;
        }
        let x = self.radius * dir.dot(self.east) / c / self.scale;
        let y = self.radius * dir.dot(self.north) / c / self.scale;
        Some(Vec2::new(x as f32, y as f32))
    }

    /// The direction under a pixel.
    pub fn to_dir(self, px: Vec2) -> DVec3 {
        let x = px.x as f64 * self.scale / self.radius;
        let y = px.y as f64 * self.scale / self.radius;
        (self.centre + self.east * x + self.north * y).normalize()
    }

    /// How far the chart reaches from its middle to a window's corner,
    /// radians.
    fn reach(&self, size: Vec2) -> f64 {
        (size.length() as f64 * 0.5 * self.scale / self.radius).atan()
    }
}

/// The map: whether it is open, where it looks and how close, and what
/// the mouse is doing to it.
#[derive(Resource)]
pub struct MapView {
    pub open: bool,
    centre: DVec3,
    /// Metres a pixel.
    scale: f64,
    /// Where the left button went down, and whether it has moved since:
    /// a press that has not moved is a click when it is let go.
    press: Option<(Vec2, bool)>,
    /// The cursor a frame ago, so a drag is read off where the cursor
    /// IS and never off motion events, which come in another unit.
    last: Option<Vec2>,
    /// The view the sea layer was last built for.
    sea_at: Option<(DVec3, f64)>,
    /// Per route, its middle and how far it reaches from it, radians,
    /// so a road nowhere near the view costs one test.
    plan: Vec<(DVec3, f64)>,
    /// Whether `--map` has opened it once.
    fired: bool,
    /// Whether the layers are showing, so closing hides them once.
    shown: bool,
}

impl Default for MapView {
    fn default() -> Self {
        MapView {
            open: false,
            centre: DVec3::Y,
            scale: 50.0,
            press: None,
            last: None,
            sea_at: None,
            plan: Vec::new(),
            fired: false,
            shown: false,
        }
    }
}

impl MapView {
    fn chart(&self, radius: f64) -> Chart {
        Chart::new(self.centre, radius, self.scale)
    }

    /// The scale the wheel may go to, metres a pixel, for a window.
    fn bounds(size: Vec2) -> (f64, f64) {
        let widest = REGION / size.x.max(1.0) as f64;
        (widest / ZOOM, widest)
    }
}

/// What says where the eye is on the body: the car at the wheel, the
/// walker, or the fly camera over the ground.
#[derive(SystemParam)]
pub struct Whereabouts<'w> {
    pub thefts: Res<'w, Thefts>,
    pub walker: Option<Res<'w, OnFoot>>,
    pub eye: Res<'w, Eye>,
    pub ground: Res<'w, Ground>,
}

impl Whereabouts<'_> {
    /// Where the eye is, as a direction: the car's, the walker's or the
    /// fly camera's own.
    pub(crate) fn here(&self) -> DVec3 {
        if let Some(theft) = self.thefts.driving() {
            theft.car.dir
        } else if let Some(w) = self.walker.as_deref() {
            w.0.dir
        } else {
            (self.eye.0 .0 - self.ground.1).normalize_or(DVec3::Y)
        }
    }

    /// The body's radius, metres.
    pub(crate) fn radius(&self) -> f64 {
        self.ground.0.planet.radius
    }
}

/// What opening the map shows and closing it hides.
type Shown<'w, 's> =
    Query<'w, 's, &'static mut Visibility, Or<(With<MapUi>, With<Dim>, With<SeaLayer>)>>;

/// What the map's own toggling touches.
#[derive(SystemParam)]
pub struct Layers<'w, 's> {
    cursor: Query<'w, 's, &'static mut CursorOptions, With<PrimaryWindow>>,
    cams: Query<'w, 's, &'static mut Camera, With<Overlay>>,
    shown: Shown<'w, 's>,
}

/// M opens and closes it, Esc closes it, and `--map` opens it once a
/// scripted drive is at the wheel. It opens centred on where the eye
/// is, at the region's own scale, and gives the mouse back, because a
/// map a player clicks on is a map the cursor has to be free for.
pub fn toggle_map(
    args: Res<Args>,
    keys: Res<ButtonInput<KeyCode>>,
    markers: Res<Markers>,
    at: Whereabouts,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut view: ResMut<MapView>,
    mut layers: Layers,
) {
    let want = if keys.just_pressed(KeyCode::KeyM) {
        Some(!view.open)
    } else if keys.just_pressed(KeyCode::Escape) && view.open {
        Some(false)
    } else if args.map && !view.fired && (args.drive == 0 || at.thefts.driving().is_some()) {
        Some(true)
    } else {
        None
    };
    let Some(open) = want else { return };
    let scripted = args.map && !view.fired;
    view.open = open;
    view.fired |= open;
    if open {
        view.centre = at.here();
        // `--map` is M and then a DRAG, which a run with no pointer
        // cannot make: it opens halfway to the first marker, so the
        // route there is in the picture and not off the edge of it.
        if let Some(first) = markers.0.first().filter(|_| scripted) {
            view.centre = (view.centre + *first).normalize_or(view.centre);
        }
        let width = windows.single().map_or(1280.0, |w| w.width());
        view.scale = REGION / width.max(1.0) as f64;
        view.press = None;
        view.last = None;
        if view.plan.is_empty() {
            view.plan = at.ground.0.routes.iter().map(|r| spread(&r.line)).collect();
        }
        if let Ok(mut cursor) = layers.cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
    for mut cam in &mut layers.cams {
        cam.is_active = open;
    }
    for mut seen in &mut layers.shown {
        *seen = if open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// A route's middle and how far it reaches from it, radians.
fn spread(line: &[DVec3]) -> (DVec3, f64) {
    let sum: DVec3 = line.iter().sum();
    let mid = sum.normalize_or(DVec3::Y);
    let reach = line
        .iter()
        .map(|p| p.angle_between(mid))
        .fold(0.0, f64::max);
    (mid, reach)
}

/// The mouse on the map: a wheel zooms about the cursor, a drag pans, a
/// click sets a marker and a right click takes the last one back.
pub fn work_map(
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ground: Res<Ground>,
    panels: Query<&Interaction, With<Panel>>,
    mut view: ResMut<MapView>,
    mut markers: ResMut<Markers>,
) {
    if !view.open {
        wheel.clear();
        view.press = None;
        view.last = None;
        return;
    }
    let Ok(window) = windows.single() else { return };
    let size = window.size();
    let cursor = window.cursor_position();
    let over = panels.iter().any(|i| *i != Interaction::None);
    let radius = ground.0.planet.radius;
    // Window pixels run down; the overlay's run up from the middle.
    let overlay = |c: Vec2| Vec2::new(c.x - size.x * 0.5, size.y * 0.5 - c.y);
    for w in wheel.read() {
        let notches = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 40.0,
        };
        if let Some(c) = cursor {
            zoom(&mut view, radius, size, overlay(c), notches as f64);
        }
    }
    if buttons.just_pressed(MouseButton::Left) && !over {
        view.press = cursor.map(|c| (c, false));
    }
    if buttons.pressed(MouseButton::Left) {
        if let (Some(c), Some(last), Some((at, moved))) = (cursor, view.last, view.press) {
            let d = c - last;
            if moved || (c - at).length() > CLICK {
                let chart = view.chart(radius);
                view.centre = chart.to_dir(Vec2::new(-d.x, d.y));
                view.press = Some((at, true));
            }
        }
    }
    if buttons.just_released(MouseButton::Left) {
        if let Some((_, moved)) = view.press.take() {
            if let (false, false, Some(c)) = (moved, over, cursor) {
                if markers.0.len() < MOST {
                    markers.0.push(view.chart(radius).to_dir(overlay(c)));
                }
            }
        }
    }
    if buttons.just_pressed(MouseButton::Right) && !over {
        markers.0.pop();
    }
    view.last = cursor;
}

/// Zoom by `notches` about a point of the overlay, so the ground under
/// the cursor stays under it: the direction there is found first, the
/// scale changed, and the centre moved by however far that direction
/// then stands from the cursor.
fn zoom(view: &mut MapView, radius: f64, size: Vec2, at: Vec2, notches: f64) {
    let (nearest, widest) = MapView::bounds(size);
    let before = view.chart(radius);
    let under = before.to_dir(at);
    view.scale = (view.scale / NOTCH.powf(notches)).clamp(nearest, widest);
    // Recentre so `under` lands back on `at`. One step is exact on a
    // plane and a hair off on the sphere, because the chart's frame turns
    // with its centre; a second closes it to under a hundredth of a pixel.
    for _ in 0..2 {
        let after = view.chart(radius);
        let Some(lands) = after.to_px(under) else {
            return;
        };
        view.centre = after.to_dir(lands - at);
    }
}

/// The route panel and the scale bar: the legs from the car through the
/// markers, their sum, what the tank holds, and a bar a round number of
/// kilometres long.
pub fn show_route(
    view: Res<MapView>,
    route: crate::route::Planned,
    at: Whereabouts,
    mut says: Query<(&mut Text, &Says)>,
    mut bar: Query<&mut Node, With<ScaleBar>>,
) {
    if !view.open {
        return;
    }
    let legs = legs_of(&route, at.here(), at.radius());
    let total: f64 = legs.iter().map(|l| l.0).sum();
    let holds = at.thefts.driving().map(|t| t.car.tank.reach());
    let (bar_m, bar_px) = scale_of(view.scale);
    for (mut line, what) in &mut says {
        line.0 = match what {
            Says::Legs if legs.is_empty() => "no markers set".to_string(),
            Says::Legs => legs
                .iter()
                .enumerate()
                .map(|(i, (d, roads))| {
                    format!(
                        "{}   {}{}",
                        i + 1,
                        km(*d),
                        if *roads { "" } else { ", no road" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Says::Total => km(total),
            Says::Holds => holds.map_or("on foot".to_string(), |m| {
                if total > m && !legs.is_empty() {
                    format!("{}, short", km(m))
                } else {
                    km(m)
                }
            }),
            Says::Scale => km(bar_m),
        };
    }
    for mut node in &mut bar {
        node.width = Val::Px(bar_px);
    }
}

/// Each leg's length and whether it follows the roads: ALONG them as
/// planned, and as the crow flies for a leg the plan has not caught up
/// with, which is a marker set this frame and is not yet known to have
/// no road, so it is not said to.
fn legs_of(route: &crate::route::Planned, here: DVec3, radius: f64) -> Vec<(f64, bool)> {
    if let Some(legs) = route.legs() {
        return legs.iter().map(|l| (l.metres, l.roads)).collect();
    }
    let mut from = here;
    route
        .markers
        .0
        .iter()
        .map(|m| {
            let d = from.angle_between(*m) * radius;
            from = *m;
            (d, true)
        })
        .collect()
}

/// A distance in kilometres, to a tenth under ten and whole past.
fn km(m: f64) -> String {
    if m < 10_000.0 {
        format!("{:.1} km", m / 1000.0)
    } else {
        format!("{:.0} km", m / 1000.0)
    }
}

/// The longest round number of metres that fits a bar of 160 px at a
/// scale, and how long that bar is.
fn scale_of(scale: f64) -> (f64, f32) {
    let steps = [
        100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0,
    ];
    let m = steps
        .iter()
        .copied()
        .filter(|s| s / scale <= 160.0)
        .fold(steps[0], f64::max);
    (m, (m / scale) as f32)
}

/// The clear button as the mouse leaves it: its state and its paint.
type Pressed<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static mut BackgroundColor),
    (Changed<Interaction>, With<Clear>),
>;

/// The clear button, pressed.
pub fn press_clear(mut markers: ResMut<Markers>, mut buttons: Pressed) {
    for (state, mut paint) in &mut buttons {
        *paint = BackgroundColor(match state {
            Interaction::Pressed => Color::srgba(0.914, 0.902, 0.847, 0.3),
            Interaction::Hovered => Color::srgba(0.914, 0.902, 0.847, 0.12),
            Interaction::None => Color::NONE,
        });
        if *state == Interaction::Pressed {
            markers.0.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chart goes both ways: a direction to a pixel and back is the
    /// direction, anywhere in the window, and a kilometre north is a
    /// kilometre up the screen at the scale.
    #[test]
    fn the_chart_goes_to_a_pixel_and_back() {
        let radius = 1_000_000.0;
        let centre = DVec3::new(0.3, 0.5, 0.81).normalize();
        let chart = Chart::new(centre, radius, 50.0);
        let (east, north) = town::frame_at(centre);
        let mut worst: f64 = 0.0;
        for i in -6..=6 {
            for j in -4..=4 {
                let d =
                    (centre + east * (i as f64 * 0.004) + north * (j as f64 * 0.004)).normalize();
                let p = chart
                    .to_px(d)
                    .expect("a direction near the middle is on the chart");
                let back = chart.to_dir(p);
                worst = worst.max(back.angle_between(d) * radius);
            }
        }
        println!("round trip worst {worst:.6} m");
        assert!(worst < 1e-6, "{worst}");
        let up = (centre * radius + north * 1000.0).normalize();
        let p = chart.to_px(up).expect("on the chart");
        println!("a kilometre north lands at {p:?}");
        assert!((p.y - 20.0).abs() < 0.01 && p.x.abs() < 0.01, "{p:?}");
        assert!(
            chart.to_px(-centre).is_none(),
            "the far side is off the chart"
        );
    }

    /// A zoom holds the ground under the cursor still, and stops at the
    /// region and at the town.
    #[test]
    fn a_zoom_holds_the_ground_under_the_cursor() {
        let radius = 1_000_000.0;
        let size = Vec2::new(1280.0, 720.0);
        let mut view = MapView {
            centre: DVec3::new(0.3, 0.5, 0.81).normalize(),
            scale: REGION / 1280.0,
            ..default()
        };
        let at = Vec2::new(300.0, -150.0);
        let under = view.chart(radius).to_dir(at);
        zoom(&mut view, radius, size, at, 3.0);
        let lands = view.chart(radius).to_px(under).expect("still on the chart");
        println!("after three notches the ground under the cursor lands at {lands:?}");
        assert!((lands - at).length() < 0.01, "{lands:?} against {at:?}");
        zoom(&mut view, radius, size, at, 100.0);
        let (nearest, widest) = MapView::bounds(size);
        assert!((view.scale - nearest).abs() < 1e-9);
        zoom(&mut view, radius, size, at, -100.0);
        assert!((view.scale - widest).abs() < 1e-9);
    }

    /// The scale bar is a round number that fits.
    #[test]
    fn the_scale_bar_is_a_round_number_that_fits() {
        for scale in [2.0, 10.0, 48.4, 200.0] {
            let (m, px) = scale_of(scale);
            println!("{scale} m/px: {m} m over {px} px");
            assert!(px <= 160.0 && px > 30.0, "{px}");
        }
        assert_eq!(km(1234.0), "1.2 km");
        assert_eq!(km(51_234.0), "51 km");
    }
}
