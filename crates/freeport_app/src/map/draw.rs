//! The map DRAWN: the overlay camera and the layers under it, the sea
//! as wet cells off the field, the roads, the settlements, the pumps,
//! the legs, the markers and the car, and the panels' nodes. The mode
//! that decides what it shows is `map.rs`, which this is a child of.

use super::{
    Chart, Clear, Dim, MapGizmos, MapUi, MapView, Overlay, Panel, Says, ScaleBar, SeaLayer,
    Whereabouts, MOST,
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
use freeport_core::road::PIECE;
use freeport_core::town::Tier;

/// The render layer the overlay lives on. Layer 1 is the LOD wireframe's.
const LAYER: usize = 2;
/// The sea is sampled on this grid over the window, cells: about
/// thirteen pixels at 1280 across, five thousand samples of the field
/// once per view rather than a rebuild a frame.
const SEA_GRID: (usize, usize) = (96, 54);
/// How wide a road's line is drawn, pixels.
const LINE: f32 = 2.0;
/// A hop's dashes and the gaps between them, pixels: the page's own
/// `setLineDash([6, 5])`.
const DASH: f32 = 6.0;
const DASH_GAP: f32 = 5.0;
/// The page's own colours for what only the map has. The dimmer's alpha
/// is not the page's 0.82: a browser composites in sRGB, where 0.82
/// leaves the drive at 18% of its brightness, and Bevy composites in
/// linear light, where the same 18% of sRGB is 2.7% and wants 0.97.
/// Measured on the first render, which left the street under the map
/// at about half its brightness.
const DIMMER: Color = Color::srgba(0.031, 0.039, 0.051, 0.97);
const SEA: Color = Color::srgba(0.16, 0.30, 0.48, 0.55);
const ROAD: Color = Color::srgb(0.55, 0.52, 0.45);

/// A settlement's label, a pump and a marker's number, each placed by
/// the drawing every frame.
#[derive(Component)]
pub struct Label(usize);
#[derive(Component)]
pub struct PumpSpot(usize);
#[derive(Component)]
pub struct MarkerLabel(usize);

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

/// The overlay camera, the layers under the map and its panels, once.
pub fn spawn_map(
    mut commands: Commands,
    ground: Res<Ground>,
    network: Res<Network>,
    mut config: ResMut<GizmoConfigStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
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
            color: DIMMER,
            custom_size: Some(Vec2::splat(20_000.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -2.0),
        layer.clone(),
        Visibility::Hidden,
        Dim,
    ));
    let sea = meshes.add(cells(&[]));
    commands.spawn((
        Mesh2d(sea.clone()),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(SEA))),
        Transform::from_xyz(0.0, 0.0, -1.0),
        layer.clone(),
        Visibility::Hidden,
        SeaLayer(sea),
    ));
    for (i, t) in ground.0.towns.iter().enumerate() {
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
    for i in 0..network.pump_sites().len() {
        commands.spawn((
            Sprite {
                color: AMBER,
                custom_size: Some(Vec2::splat(7.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 1.0),
            layer.clone(),
            Visibility::Hidden,
            PumpSpot(i),
        ));
    }
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
        ));
    }
    spawn_panels(&mut commands, overlay);
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
        panel.spawn((text("no markers set", 13.0, DIM), Says::Legs));
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

/// The sea layer's mesh: a quad a wet cell, in overlay pixels.
fn cells(wet: &[(Vec2, Vec2)]) -> Mesh {
    let mut positions = Vec::with_capacity(wet.len() * 4 + 3);
    let mut indices = Vec::with_capacity(wet.len() * 6 + 3);
    for (i, (at, size)) in wet.iter().enumerate() {
        let (x0, y0, x1, y1) = (at.x, at.y, at.x + size.x, at.y + size.y);
        positions.extend([[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]]);
        let k = (i * 4) as u32;
        indices.extend([k, k + 1, k + 2, k, k + 2, k + 3]);
    }
    if wet.is_empty() {
        // A mesh with nothing in it is refused, so an empty sea is one
        // triangle of no area.
        positions.extend([[0.0; 3], [0.0; 3], [0.0; 3]]);
        indices.extend([0, 1, 2]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// The sea, as the cells of a grid over the window whose middle is
/// under the water, rebuilt when the view has moved and the drag has
/// let go.
pub fn sea_layer(
    ground: Res<Ground>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut view: ResMut<MapView>,
    layer: Query<&SeaLayer>,
) {
    if !view.open || view.press.is_some_and(|(_, moved)| moved) {
        return;
    }
    if view.sea_at == Some((view.centre, view.scale)) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok(sea) = layer.single() else { return };
    let size = window.size();
    let world = &ground.0;
    let chart = view.chart(world.planet.radius);
    let cell = Vec2::new(size.x / SEA_GRID.0 as f32, size.y / SEA_GRID.1 as f32);
    let mut wet = Vec::new();
    for j in 0..SEA_GRID.1 {
        for i in 0..SEA_GRID.0 {
            let at = Vec2::new(
                -size.x * 0.5 + cell.x * i as f32,
                -size.y * 0.5 + cell.y * j as f32,
            );
            let dir = chart.to_dir(at + cell * 0.5);
            let ground_r = world.planet.radius + world.planet.surface(dir).0;
            if ground_r < world.sea.radius {
                wet.push((at, cell));
            }
        }
    }
    if let Err(e) = meshes.insert(sea.0.id(), cells(&wet)) {
        warn!("the map's sea layer could not be written: {e:?}");
    }
    view.sea_at = Some((view.centre, view.scale));
}

/// A thing the drawing PLACES, its transform and whether it shows.
type Placing<'w, 's, T, F> =
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility, &'static T), F>;

/// What the drawing places: the labels, the pumps and the markers'
/// numbers.
#[derive(SystemParam)]
pub struct Placed<'w, 's> {
    labels: Placing<'w, 's, Label, ()>,
    pumps: Placing<'w, 's, PumpSpot, Without<Label>>,
    numbers: Placing<'w, 's, MarkerLabel, (Without<Label>, Without<PumpSpot>)>,
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
    }
}

/// Where a thing on the map stands, if it is in the window at all.
fn inside(chart: &Chart, dir: DVec3, half: Vec2, slack: f32) -> Option<Vec2> {
    chart
        .to_px(dir)
        .filter(|p| p.x.abs() <= half.x + slack && p.y.abs() <= half.y + slack)
}

/// How big a settlement's mark is, pixels: a city, a town, a village.
fn mark_of(radius: f64) -> f32 {
    match Tier::of(radius) {
        Tier::City => 7.0,
        Tier::Town => 5.0,
        Tier::Village => 3.0,
    }
}

/// The map, drawn: the roads, the car's own road brighter, the towns,
/// the pumps, the legs and the markers, and the car.
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
    let on = network.nearest_road(world, here, at.radius());
    draw_roads(
        &mut gizmos,
        &chart,
        &view,
        world,
        on,
        half,
        chart.reach(window.size()),
    );
    draw_towns(&mut gizmos, &chart, world, half, &mut placed);
    place_pumps(&chart, network.pump_sites(), half, &mut placed);
    draw_route(&mut gizmos, &chart, here, &route, half, &mut placed);
    draw_car(&mut gizmos, &chart, here, &at.thefts);
}

/// Every road the view can reach, and the car's own last and brighter.
fn draw_roads(
    gizmos: &mut Gizmos<MapGizmos>,
    chart: &Chart,
    view: &MapView,
    world: &World,
    on: Option<usize>,
    half: Vec2,
    reach: f64,
) {
    for (r, route) in world.routes.iter().enumerate() {
        if Some(r) == on {
            continue;
        }
        if let Some((mid, spread)) = view.plan.get(r) {
            if mid.angle_between(chart.centre) > spread + reach {
                continue;
            }
        }
        road(gizmos, chart, &route.line, half, ROAD);
    }
    if let Some(r) = on {
        road(gizmos, chart, &world.routes[r].line, half, HUD);
    }
}

/// A settlement is two rings at its tier's size, the port ringed again
/// in the route's colour, with its label beside it.
fn draw_towns(
    gizmos: &mut Gizmos<MapGizmos>,
    chart: &Chart,
    world: &World,
    half: Vec2,
    placed: &mut Placed,
) {
    for (i, t) in world.towns.iter().enumerate() {
        let r = mark_of(t.radius);
        if let Some(p) = inside(chart, t.dir, half, 40.0) {
            let iso = Isometry2d::from_translation(p);
            gizmos.circle_2d(iso, r, HUD);
            gizmos.circle_2d(iso, r * 0.5, HUD);
            if i == 0 {
                gizmos.circle_2d(iso, r + 6.0, ROUTE);
            }
        }
    }
    for (mut tf, mut seen, label) in &mut placed.labels {
        let t = &world.towns[label.0];
        match inside(chart, t.dir, half, 40.0) {
            Some(p) => {
                tf.translation = Vec3::new(p.x + mark_of(t.radius) + 6.0, p.y, 2.0);
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
                gizmos.circle_2d(Isometry2d::from_translation(p), 8.0, ROUTE);
                gizmos.circle_2d(Isometry2d::from_translation(p), 6.5, ROUTE);
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

/// The car as an arrow along its own heading in a ring, or the eye as a
/// dot in one when nobody is at a wheel.
fn draw_car(gizmos: &mut Gizmos<MapGizmos>, chart: &Chart, here: DVec3, thefts: &Thefts) {
    let Some(p) = chart.to_px(here) else { return };
    let iso = Isometry2d::from_translation(p);
    match thefts.driving() {
        Some(theft) => {
            let b = hud::bearing(theft.car.dir, theft.car.fwd).to_radians();
            let h = Vec2::new(b.sin() as f32, b.cos() as f32);
            gizmos.arrow_2d(p - h * 9.0, p + h * 11.0, AMBER);
        }
        None => {
            gizmos.circle_2d(iso, 5.0, AMBER);
        }
    }
    gizmos.circle_2d(iso, 14.0, AMBER);
}

/// One road's line, at a stride that leaves about a point a pixel and a
/// half, only where either end is in the window.
fn road(gizmos: &mut Gizmos<MapGizmos>, chart: &Chart, line: &[DVec3], half: Vec2, colour: Color) {
    let stride = ((1.5 * chart.scale / PIECE) as usize).max(1);
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
