//! The map's PICTURE: the core's own rendering of the ground from above
//! (`freeport_core::map`) fed from the world the harness built, drawn on
//! a thread of its own whenever the view has moved and laid under the
//! markers as one sprite, and the headless run that writes one to a file.
//!
//! A picture is a third of a second of work on four cores at 1280 by
//! 720, one sample of the planet a pixel, so it is never drawn in the
//! frame: a pan or a zoom moves and scales the LAST picture under the
//! cursor at once, and the new one replaces it when it lands. A
//! `JoinHandle` and not a channel, for the sky's own reason: there is
//! one answer and no queue, and `is_finished` is the poll.

use super::MapView;
use crate::world::{Ground, World};
use crate::Args;
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use freeport_core::field::Planet;
use freeport_core::map::{self, Line, Picture, Scene, View};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// What a picture was drawn OF: where it looks, how close, and how many
/// pixels, which is everything that says whether it is the picture the
/// view wants now.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Framing {
    centre: DVec3,
    /// Metres a pixel.
    scale: f64,
    size: UVec2,
}

/// The map's picture: the image it is shown from, what that image was
/// drawn for, and the one being drawn.
#[derive(Resource)]
pub struct Relief {
    image: Handle<Image>,
    shown: Option<Framing>,
    drawing: Option<(Framing, JoinHandle<(Picture, f64)>)>,
    /// The planet with no sites in it, stripped once: a body carries
    /// seven hundred thousand of them and a picture asks for none.
    bare: Option<Arc<Planet>>,
}

impl Relief {
    /// A picture of nothing yet, on an image of one dark pixel.
    pub fn new(images: &mut Assets<Image>) -> Relief {
        Relief {
            image: images.add(image_of(1, 1, vec![11, 13, 16, 255])),
            shown: None,
            drawing: None,
            bare: None,
        }
    }

    /// The image the sprite shows.
    pub fn image(&self) -> Handle<Image> {
        self.image.clone()
    }
}

/// The sprite the picture is shown on.
#[derive(Component)]
pub struct ReliefSprite;

/// An eight bit sRGB image of a picture's own bytes.
fn image_of(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The map of a world in a view, `width` by `height`: its bare planet
/// shaded, tinted and contoured, its roads on their tarmac, its towns'
/// streets and buildings.
pub(crate) fn picture(
    world: &World,
    bare: &Planet,
    view: &View,
    width: usize,
    height: usize,
) -> Picture {
    let lines: Vec<Line> = world
        .routes
        .iter()
        .map(|r| Line {
            points: &r.line,
            open: &r.open,
        })
        .collect();
    let scene = Scene {
        planet: bare,
        sea: world.sea.radius,
        roads: &lines,
        towns: &world.towns,
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    map::draw(&scene, view, width, height, threads)
}

/// Take the picture that has landed, and start the next one when the
/// view has moved from the one shown: the picture is always of the view
/// as it stood when it was started, and a drag that outruns it is drawn
/// again the moment it lands.
pub fn draw_relief(
    view: Res<MapView>,
    ground: Res<Ground>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut relief: ResMut<Relief>,
    mut images: ResMut<Assets<Image>>,
) {
    if relief
        .drawing
        .as_ref()
        .is_some_and(|(_, j)| !j.is_finished())
    {
        return;
    }
    if let Some((framing, job)) = relief.drawing.take() {
        // A worker that panicked leaves the last picture, which is a map
        // a pan behind rather than no map at all.
        match job.join() {
            Ok((pic, ms)) => {
                let image = image_of(pic.width as u32, pic.height as u32, pic.rgba);
                match images.insert(&relief.image, image) {
                    Ok(()) => relief.shown = Some(framing),
                    Err(e) => warn!("the map's picture could not be shown: {e}"),
                }
                info!(
                    "map: {} by {} at {:.1} m a pixel drawn in {ms:.0} ms",
                    framing.size.x, framing.size.y, framing.scale
                );
            }
            Err(_) => warn!("the map's picture was not drawn"),
        }
    }
    if !view.open {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let want = Framing {
        centre: view.centre,
        scale: view.scale,
        size: window.size().as_uvec2().max(UVec2::ONE),
    };
    if relief.shown == Some(want) {
        return;
    }
    let world = ground.0.clone();
    let bare = relief
        .bare
        .get_or_insert_with(|| Arc::new(world.planet.bare()))
        .clone();
    let job = std::thread::spawn(move || {
        let t0 = Instant::now();
        let view = View::new(want.centre, world.planet.radius, want.scale);
        let (w, h) = (want.size.x as usize, want.size.y as usize);
        let pic = picture(&world, &bare, &view, w, h);
        (pic, t0.elapsed().as_secs_f64() * 1000.0)
    });
    relief.drawing = Some((want, job));
}

/// Lay the picture shown where the view now puts it: its own middle at
/// the pixel its centre lands on, and its size by the ratio of the scale
/// it was drawn at to the scale the view is at, so a pan or a zoom moves
/// the old picture with the cursor until the new one lands.
pub fn place_relief(
    view: Res<MapView>,
    ground: Res<Ground>,
    relief: Res<Relief>,
    mut sprites: Query<(&mut Transform, &mut Sprite), With<ReliefSprite>>,
) {
    if !view.open {
        return;
    }
    let chart = view.chart(ground.0.planet.radius);
    for (mut tf, mut sprite) in &mut sprites {
        let lay = relief.shown.and_then(|shown| {
            let at = chart.to_px(shown.centre)?;
            Some((at, shown.size.as_vec2() * (shown.scale / view.scale) as f32))
        });
        match lay {
            Some((at, size)) => {
                tf.translation = Vec3::new(at.x, at.y, tf.translation.z);
                sprite.custom_size = Some(size);
            }
            None => sprite.custom_size = Some(Vec2::ZERO),
        }
    }
}

/// Whether the picture under the map is the view's own and nothing is
/// being drawn, so a screenshot of the map waits for its ground.
pub fn relief_ready(view: &MapView, relief: &Relief) -> bool {
    !view.open
        || (relief.drawing.is_none()
            && relief
                .shown
                .is_some_and(|s| s.centre == view.centre && s.scale == view.scale))
}

/// The map drawn with no window and written out, `--map-png PATH` at
/// `--map-scale` metres a pixel over the port: a picture of the map is a
/// function of the world and nothing on the GPU, so it is taken the way
/// the atlas is baked, before any of Bevy is built, and says what it did
/// on the terminal for the same reason: there is no logger yet.
pub(crate) fn draw_headless(args: &Args, path: &str) {
    let world = crate::world::build(args);
    let Some(port) = world.towns.first() else {
        eprintln!("no towns to draw a map over");
        return;
    };
    let centre = args.eye.map_or(port.dir, |e| e.normalize_or(port.dir));
    let view = View::new(centre, world.planet.radius, args.map_scale);
    let bare = world.planet.bare();
    let t0 = Instant::now();
    let pic = picture(&world, &bare, &view, 1280, 720);
    println!(
        "map at {} m a pixel drawn in {:.0} ms",
        args.map_scale,
        t0.elapsed().as_secs_f64() * 1000.0
    );
    let rgb: Vec<u8> = pic
        .rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    match image::RgbImage::from_raw(pic.width as u32, pic.height as u32, rgb).map(|b| b.save(path))
    {
        Some(Ok(())) => println!("map written to {path}"),
        Some(Err(e)) => eprintln!("map not written: {e}"),
        None => eprintln!("map not written: a picture of the wrong size"),
    }
}
