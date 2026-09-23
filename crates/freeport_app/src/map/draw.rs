//! The map DRAWN over its picture: the overlay camera, the page and the
//! picture under everything, the car's own road, the route, the markers,
//! the pumps, the labels and the car, and the panels' nodes. The ground,
//! the sea, the roads and the towns are the PICTURE's (`map/raster.rs`);
//! what is here is what only a map has. The mode that decides what it
//! shows is `map.rs`, which this is a child of.

use super::{
    Chart, Clear, Dim, MapGizmos, MapUi, MapView, Overlay, Panel, Relief, ReliefSprite, Row, Says,
    ScaleBar, Whereabouts, MOST,
};
use crate::drive::Thefts;
use crate::hud::{self, AMBER, DIM, EDGE, GLASS, HUD, ROUTE};
use crate::roads::Network;
use crate::route::Planned;
use crate::world::{Ground, World};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::ui::Val2;
use bevy::window::PrimaryWindow;
use freeport_core::map;
use freeport_core::road::PIECE;
use freeport_core::town::Tier;
use freeport_core::town::OUTLINE;

/// The render layer the overlay lives on. Layer 1 is the LOD wireframe's.
const LAYER: usize = 2;
/// How wide a road's line is drawn, pixels.
const LINE: f32 = 2.0;
/// A hop's dashes and the gaps between them, pixels: the page's own
/// `setLineDash([6, 5])`.
const DASH: f32 = 6.0;
const DASH_GAP: f32 = 5.0;
/// The page the picture is laid on, the page's own screen `#0b0d10`,
/// OPAQUE: a map is read, and a drive showing through it is a map that
/// cannot be. It is also what a pan uncovers at the picture's edge until
/// the next picture lands.
const PAGE: Color = Color::srgb(0.043, 0.051, 0.063);
/// A marker's disc and a pump's mark, pixels: the page's own.
const MARKER: f32 = 8.0;
const PUMP: f32 = 8.0;
/// How far out from a place's mark its ring and its label stand, pixels,
/// and the largest a town is ringed at: past it the town is its own plan
/// filling the window and a ring round it is nothing anybody can find.
const RING_GAP: f32 = 6.0;
const RING_MOST: f32 = 60.0;

/// A settlement's label, a pump and a marker's number, each placed by
/// the drawing every frame.
#[derive(Component)]
pub struct Label(usize);
#[derive(Component)]
pub struct PumpSpot(usize);
#[derive(Component)]
pub struct MarkerLabel(usize);
/// The car's own arrow.
#[derive(Component)]
pub struct CarMark;

/// A line of text at a size and a colour, on the overlay's own UI.
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

/// A glass panel, like the HUD's.
fn glass(node: Node) -> impl Bundle {
    (
        Node {
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..node
        },
        BackgroundColor(GLASS),
        BorderColor::all(EDGE),
    )
}

/// The overlay camera, the page and the picture under the map, the marks
/// over it and its panels, once.
pub fn spawn_map(
    mut commands: Commands,
    ground: Res<Ground>,
    network: Res<Network>,
    mut config: ResMut<GizmoConfigStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (cfg, _) = config.config_mut::<MapGizmos>();
    cfg.render_layers = RenderLayers::layer(LAYER);
    cfg.line.width = LINE;
    let layer = RenderLayers::layer(LAYER);
    let overlay = commands
        .spawn((
            Camera2d,
            Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                is_active: false,
                ..default()
            },
            layer.clone(),
            Overlay,
        ))
        .id();
    commands.spawn((
        Sprite {
            color: PAGE,
            custom_size: Some(Vec2::splat(20_000.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -2.0),
        layer.clone(),
        Visibility::Hidden,
        Dim,
    ));
    let relief = Relief::new(&mut images);
    commands.spawn((
        Sprite {
            image: relief.image(),
            custom_size: Some(Vec2::ZERO),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -1.0),
        layer.clone(),
        Visibility::Hidden,
        ReliefSprite,
    ));
    commands.insert_resource(relief);
    spawn_labels(&mut commands, &ground.0, &layer);
    spawn_pumps(&mut commands, network.pump_sites().len(), &layer);
    let disc = meshes.add(Circle::new(MARKER));
    let cyan = materials.add(ColorMaterial::from_color(ROUTE));
    for i in 0..MOST {
        commands.spawn((
            Text2d::new((i + 1).to_string()),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.04, 0.05, 0.06)),
            Anchor::CENTER,
            Transform::from_xyz(0.0, 0.0, 3.0),
            layer.clone(),
            Visibility::Hidden,
            MarkerLabel(i),
            children![(
                Mesh2d(disc.clone()),
                MeshMaterial2d(cyan.clone()),
                Transform::from_xyz(0.0, 0.0, -0.5),
                layer.clone(),
            )],
        ));
    }
    commands.spawn((
        Mesh2d(meshes.add(arrow())),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(AMBER))),
        Transform::from_xyz(0.0, 0.0, 4.0),
        layer.clone(),
        Visibility::Hidden,
        CarMark,
    ));
    spawn_panels(&mut commands, overlay);
}

/// A settlement's name beside it: the port, a city in the readout's
/// white, a town and a village dim.
fn spawn_labels(commands: &mut Commands, world: &World, layer: &RenderLayers) {
    for (i, t) in world.towns.iter().enumerate() {
        let (name, colour) = match (i, Tier::of(t.radius)) {
            (0, _) => ("the port".to_string(), HUD),
            (_, Tier::City) => (format!("city {i}"), HUD),
            (_, Tier::Town) => (format!("town {i}"), DIM),
            (_, Tier::Village) => (format!("village {i}"), DIM),
        };
        commands.spawn((
            Text2d::new(name),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(colour),
            Anchor::CENTER_LEFT,
            Transform::from_xyz(0.0, 0.0, 2.0),
            layer.clone(),
            Visibility::Hidden,
            Label(i),
        ));
    }
}

/// A pump, the page's own mark: an amber square with the page showing
/// through its middle, so it reads as a place and never as a blot.
fn spawn_pumps(commands: &mut Commands, count: usize, layer: &RenderLayers) {
    for i in 0..count {
        commands.spawn((
            Sprite {
                color: AMBER,
                custom_size: Some(Vec2::splat(PUMP)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 1.0),
            layer.clone(),
            Visibility::Hidden,
            PumpSpot(i),
            children![(
                Sprite {
                    color: PAGE,
                    custom_size: Some(Vec2::splat(PUMP * 0.4)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 0.1),
                layer.clone(),
            )],
        ));
    }
}

/// The car's arrow, pointing up the screen with its tip at the car: the
/// page's own filled arrowhead, a notch at its tail so which way it
/// points reads at a glance.
fn arrow() -> Mesh {
    let (tip, wing, tail, notch) = (11.0, 7.0, -8.0, -3.0);
    let positions = vec![
        [0.0, tip, 0.0],
        [-wing, tail, 0.0],
        [0.0, notch, 0.0],
        [wing, tail, 0.0],
    ];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// The panels over the map: the title, the north, the route, the legend
/// and the scale bar, on the overlay camera's own UI.
fn spawn_panels(commands: &mut Commands, overlay: Entity) {
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
            Pickable::IGNORE,
            UiTargetCamera(overlay),
            Visibility::Hidden,
            MapUi,
        ))
        .with_children(|ui| {
            ui.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(3.5),
                    ..default()
                },
                UiTransform::from_translation(Val2 {
                    x: Val::Percent(-50.0),
                    y: Val::ZERO,
                }),
                children![text(
                    "THE MAP.  CLICK TO SET A MARKER,  WHEEL TO ZOOM,  DRAG TO PAN",
                    12.0,
                    DIM
                )],
            ));
            ui.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Percent(2.2),
                    top: Val::Percent(1.0),
                    ..default()
                },
                children![text("N", 12.0, HUD)],
            ));
            route_panel(ui);
            legend(ui);
            scale_bar(ui);
        });
}

/// The route panel: the legs, the total, what the tank holds, and the
/// button that clears them.
fn route_panel(ui: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    ui.spawn((
        glass(Node {
            position_type: PositionType::Absolute,
            right: Val::Percent(2.2),
            top: Val::Percent(3.5),
            min_width: Val::Px(200.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        }),
        Interaction::None,
        Panel,
    ))
    .with_children(|panel| {
        panel.spawn(text("ROUTE", 10.0, DIM));
        panel.spawn((text("no markers set", 13.0, DIM), Row::NoLegs));
        for i in 0..MOST {
            panel.spawn((
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(12.0),
                    display: Display::None,
                    ..default()
                },
                Row::Leg(i),
                children![
                    (text("", 13.0, ROUTE), Says::LegName(i)),
                    (text("", 13.0, HUD), Says::LegKm(i)),
                ],
            ));
        }
        for (name, what) in [("total", Says::Total), ("tank holds", Says::Holds)] {
            panel.spawn((
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(12.0),
                    border: UiRect::top(Val::Px(1.0)),
                    padding: UiRect::top(Val::Px(4.0)),
                    ..default()
                },
                BorderColor::all(EDGE),
                children![text(name, 13.0, HUD), (text("", 13.0, HUD), what)],
            ));
        }
        panel.spawn((
            Button,
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(EDGE),
            Panel,
            Clear,
            children![text("Clear markers", 12.0, HUD)],
        ));
    });
}

/// The legend, bottom left.
fn legend(ui: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    ui.spawn((
        glass(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(2.2),
            bottom: Val::Percent(3.5),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        }),
        Interaction::None,
        Panel,
    ))
    .with_children(|col| {
        for (key, what) in [
            ("Road", "the highways, with their pumps"),
            ("Town", "a settlement, the port ringed"),
            ("Marker", "a stop you set, in order"),
            ("Right click", "removes the last marker"),
        ] {
            col.spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                children![text(key, 12.0, HUD), text(what, 12.0, DIM)],
            ));
        }
    });
}

/// The scale bar, bottom right: a bar a round number of kilometres long.
fn scale_bar(ui: &mut RelatedSpawnerCommands<'_, ChildOf>) {
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Percent(2.2),
            bottom: Val::Percent(3.5),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            (text("", 12.0, HUD), Says::Scale),
            (
                Node {
                    width: Val::Px(80.0),
                    height: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(HUD),
                ScaleBar,
            )
        ],
    ));
}

/// A thing the drawing PLACES, its transform and whether it shows.
type Placing<'w, 's, T, F> =
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility, &'static T), F>;

/// The car's arrow is none of the other things the drawing places.
type NotPlaced = (Without<Label>, Without<PumpSpot>, Without<MarkerLabel>);

/// What the drawing places: the labels, the pumps, the markers' numbers
/// and the car's arrow.
#[derive(SystemParam)]
pub struct Placed<'w, 's> {
    labels: Placing<'w, 's, Label, ()>,
    pumps: Placing<'w, 's, PumpSpot, Without<Label>>,
    numbers: Placing<'w, 's, MarkerLabel, (Without<Label>, Without<PumpSpot>)>,
    car: Placing<'w, 's, CarMark, NotPlaced>,
}

impl Placed<'_, '_> {
    /// Everything off the map.
    fn hide(&mut self) {
        for (_, mut seen, _) in &mut self.labels {
            *seen = Visibility::Hidden;
        }
        for (_, mut seen, _) in &mut self.pumps {
            *seen = Visibility::Hidden;
        }
        for (_, mut seen, _) in &mut self.numbers {
            *seen = Visibility::Hidden;
        }
        for (_, mut seen, _) in &mut self.car {
            *seen = Visibility::Hidden;
        }
    }
}

/// Where a thing on the map stands, if it is in the window at all.
fn inside(chart: &Chart, dir: DVec3, half: Vec2, slack: f32) -> Option<Vec2> {
    chart
        .to_px(dir)
        .filter(|p| p.x.abs() <= half.x + slack && p.y.abs() <= half.y + slack)
}

/// How far a settlement reaches on the map, pixels: its own outline at
/// the scale, or the picture's own mark for its tier where the outline
/// would be smaller, which is exactly what the picture drew there.
fn reach_px(town: &freeport_core::town::Town, scale: f64) -> f32 {
    let outline = town.radius * OUTLINE / scale;
    outline.max(map::mark(Tier::of(town.radius))) as f32
}

/// The map, drawn over its picture: the car's own road brighter, the
/// port's ring and every label, the pumps, the route and the markers,
/// and the car.
pub fn draw_map(
    view: Res<MapView>,
    route: Planned,
    network: Res<Network>,
    at: Whereabouts,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut gizmos: Gizmos<MapGizmos>,
    mut placed: Placed,
) {
    if !view.open {
        if view.shown {
            placed.hide();
        }
        return;
    }
    let Ok(window) = windows.single() else { return };
    let half = window.size() * 0.5;
    let world = &at.ground.0;
    let chart = view.chart(at.radius());
    let here = at.here();
    // The picture has every road; the one the car is on is drawn again
    // over it, brighter, which is the page's own rule.
    if let Some(r) = network.nearest_road(world, here, at.radius()) {
        road(&mut gizmos, &chart, &world.routes[r].line, half, HUD);
    }
    draw_towns(&mut gizmos, &chart, world, half, &mut placed);
    place_pumps(&chart, network.pump_sites(), half, &mut placed);
    draw_route(&mut gizmos, &chart, here, &route, half, &mut placed);
    draw_car(&mut gizmos, &chart, here, &at.thefts, &mut placed);
}

/// Every settlement's label beside it, past its own reach on the map,
/// and the port ringed in the route's colour while it is small enough
/// for a ring to say where it is.
fn draw_towns(
    gizmos: &mut Gizmos<MapGizmos>,
    chart: &Chart,
    world: &World,
    half: Vec2,
    placed: &mut Placed,
) {
    if let Some(port) = world.towns.first() {
        let r = reach_px(port, chart.scale());
        if let Some(p) = inside(chart, port.dir, half, 40.0).filter(|_| r < RING_MOST) {
            gizmos.circle_2d(Isometry2d::from_translation(p), r + RING_GAP, ROUTE);
        }
    }
    for (mut tf, mut seen, label) in &mut placed.labels {
        let t = &world.towns[label.0];
        match inside(chart, t.dir, half, 40.0) {
            Some(p) => {
                let off = reach_px(t, chart.scale()).min(RING_MOST) + RING_GAP * 2.0;
                tf.translation = Vec3::new(p.x + off, p.y, 2.0);
                *seen = Visibility::Inherited;
            }
            None => *seen = Visibility::Hidden,
        }
    }
}

/// The pumps, each a sprite put where its forecourt is.
fn place_pumps(chart: &Chart, pumps: &[DVec3], half: Vec2, placed: &mut Placed) {
    for (mut tf, mut seen, pump) in &mut placed.pumps {
        match inside(chart, pumps[pump.0].normalize_or(DVec3::Y), half, 10.0) {
            Some(p) => {
                tf.translation = Vec3::new(p.x, p.y, 1.0);
                *seen = Visibility::Inherited;
            }
            None => *seen = Visibility::Hidden,
        }
    }
}

/// The route: every leg the plan has, along the roads it takes, and
/// the markers numbered in the order they were set. A leg the plan has
/// not caught up with yet (a marker set this frame) is drawn straight,
/// which is what every leg was before there was a way to find.
fn draw_route(
    gizmos: &mut Gizmos<MapGizmos>,
    chart: &Chart,
    here: DVec3,
    route: &Planned,
    half: Vec2,
    placed: &mut Placed,
) {
    let markers = &*route.markers;
    match route.legs() {
        Some(legs) => {
            for leg in legs {
                draw_leg(gizmos, chart, &leg.points, half);
            }
        }
        None => {
            let mut from = here;
            for m in &markers.0 {
                dashed(gizmos, chart, from, *m, half);
                from = *m;
            }
        }
    }
    for (mut tf, mut seen, number) in &mut placed.numbers {
        match markers.0.get(number.0).and_then(|m| chart.to_px(*m)) {
            Some(p) => {
                tf.translation = Vec3::new(p.x, p.y, 3.0);
                *seen = Visibility::Inherited;
            }
            None => *seen = Visibility::Hidden,
        }
    }
}

/// One leg: SOLID in the route's colour wherever it is on tarmac, over
/// the road it follows, and DASHED across a hop, which is the page's own
/// mark for a line that follows no road: onto the road from wherever
/// the car is, across a town between two roads, and off it to a marker.
fn draw_leg(gizmos: &mut Gizmos<MapGizmos>, chart: &Chart, points: &[(DVec3, bool)], half: Vec2) {
    let mut k = 0;
    while k + 1 < points.len() {
        if !points[k + 1].1 {
            dashed(gizmos, chart, points[k].0, points[k + 1].0, half);
            k += 1;
            continue;
        }
        let mut j = k + 1;
        while j + 1 < points.len() && points[j + 1].1 {
            j += 1;
        }
        let run: Vec<DVec3> = points[k..=j].iter().map(|p| p.0).collect();
        road(gizmos, chart, &run, half, ROUTE);
        k = j;
    }
}

/// A dashed line between two places on the chart, cut to the window so
/// a leg across a sea costs its visible dashes and no more.
fn dashed(gizmos: &mut Gizmos<MapGizmos>, chart: &Chart, a: DVec3, b: DVec3, half: Vec2) {
    let (Some(p), Some(q)) = (chart.to_px(a), chart.to_px(b)) else {
        return;
    };
    let Some((p, q)) = clip(p, q, half + Vec2::splat(4.0)) else {
        return;
    };
    let long = p.distance(q);
    if long <= 0.0 {
        return;
    }
    let along = (q - p) / long;
    let mut t = 0.0;
    while t < long {
        let end = (t + DASH).min(long);
        gizmos.line_2d(p + along * t, p + along * end, ROUTE);
        t += DASH + DASH_GAP;
    }
}

/// A segment cut to a box about the middle of the window, or nothing
/// when none of it is inside: Liang and Barsky's clip.
fn clip(p: Vec2, q: Vec2, half: Vec2) -> Option<(Vec2, Vec2)> {
    let d = q - p;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (dp, lo, hi) in [
        (d.x, p.x + half.x, half.x - p.x),
        (d.y, p.y + half.y, half.y - p.y),
    ] {
        for (num, den) in [(lo, -dp), (hi, dp)] {
            if den == 0.0 {
                if num < 0.0 {
                    return None;
                }
            } else {
                let r = num / den;
                if den < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
    }
    (t0 <= t1).then(|| (p + d * t0, p + d * t1))
}

/// The car as the page's filled arrow along its own heading, or the
/// eye as a ring with a dot in it when nobody is at a wheel.
fn draw_car(
    gizmos: &mut Gizmos<MapGizmos>,
    chart: &Chart,
    here: DVec3,
    thefts: &Thefts,
    placed: &mut Placed,
) {
    let p = chart.to_px(here);
    let driving = thefts.driving();
    for (mut tf, mut seen, _) in &mut placed.car {
        match (p, driving) {
            (Some(p), Some(theft)) => {
                // A bearing is clockwise from north and the screen turns
                // anticlockwise, so the arrow is turned by the negative.
                let b = hud::bearing(theft.car.dir, theft.car.fwd).to_radians() as f32;
                *tf = Transform::from_xyz(p.x, p.y, 4.0).with_rotation(Quat::from_rotation_z(-b));
                *seen = Visibility::Inherited;
            }
            _ => *seen = Visibility::Hidden,
        }
    }
    if let (Some(p), None) = (p, driving) {
        let iso = Isometry2d::from_translation(p);
        gizmos.circle_2d(iso, 5.0, AMBER);
        gizmos.circle_2d(iso, 10.0, AMBER);
    }
}

/// One road's line, at a stride that leaves about a point a pixel and a
/// half, only where either end is in the window.
fn road(gizmos: &mut Gizmos<MapGizmos>, chart: &Chart, line: &[DVec3], half: Vec2, colour: Color) {
    let stride = ((1.5 * chart.scale() / PIECE) as usize).max(1);
    let slack = half + Vec2::splat(4.0);
    let mut prev: Option<Vec2> = None;
    let last = line.len().saturating_sub(1);
    for k in (0..line.len()).step_by(stride).chain(std::iter::once(last)) {
        let p = chart.to_px(line[k]);
        if let (Some(a), Some(b)) = (prev, p) {
            let seen = |q: Vec2| q.x.abs() <= slack.x && q.y.abs() <= slack.y;
            if seen(a) || seen(b) {
                gizmos.line_2d(a, b, colour);
            }
        }
        prev = p;
    }
}
