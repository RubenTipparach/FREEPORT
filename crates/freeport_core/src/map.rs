//! The MAP a player reads: the ground seen from straight above, DRAWN.
//!
//! A map is a picture of the world and not a diagram laid over the drive,
//! so it is rendered from the same things the world is built from: the
//! planet's own heights, shaded, tinted and contoured, the sea by its depth, every
//! road off the line its tarmac is laid on, and every town's streets and
//! buildings at their own size and place. What the player sees on it is
//! what is on the ground.
//!
//! It is in the CORE because it is a function of the world and nothing
//! else, it is tested without an engine, and the projection it is drawn
//! in is the one the harness turns a click back into a place with: one
//! `View`, so a pixel of the picture and a marker set on it cannot
//! disagree about where they are.

use crate::field::Planet;
use crate::town::{frame_at, Tier, Town, LOT, OUTLINE, PITCH};
use glam::{DVec2, DVec3};
use std::collections::BTreeSet;

mod paint;
pub use paint::{contour, mark, Picture};

/// The ground seen from straight over a place: the GNOMONIC projection
/// about `centre`, east across and north up, `scale` metres a pixel at
/// the middle. Exact both ways, so a pixel is turned back into a place
/// on the sphere without a search, and a great circle is a straight line
/// on it, which is what lets a road be drawn as the chords it is laid in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub centre: DVec3,
    east: DVec3,
    north: DVec3,
    pub radius: f64,
    pub scale: f64,
}

impl View {
    /// The view over a direction at a scale, metres a pixel.
    pub fn new(centre: DVec3, radius: f64, scale: f64) -> View {
        let centre = centre.normalize_or(DVec3::Y);
        let (east, north) = frame_at(centre);
        View {
            centre,
            east,
            north,
            radius,
            scale,
        }
    }

    /// Where a direction lands, pixels from the middle with north UP, or
    /// nothing past the projection's own horizon (a fifth of the way to
    /// the edge of the hemisphere, where it stretches past any use).
    pub fn to_px(&self, dir: DVec3) -> Option<DVec2> {
        let c = dir.dot(self.centre);
        if c < 0.2 {
            return None;
        }
        let k = self.radius / c / self.scale;
        Some(DVec2::new(dir.dot(self.east) * k, dir.dot(self.north) * k))
    }

    /// The direction under a pixel, pixels from the middle with north up.
    pub fn to_dir(&self, px: DVec2) -> DVec3 {
        let k = self.scale / self.radius;
        (self.centre + self.east * (px.x * k) + self.north * (px.y * k)).normalize()
    }

    /// How far the view reaches from its middle to a corner of a picture
    /// `size` pixels across, radians: what a road or a town has to be
    /// within to be looked at at all.
    pub fn reach(&self, size: DVec2) -> f64 {
        (size.length() * 0.5 * self.scale / self.radius).atan()
    }
}

/// A road as the map reads it: the line it is drawn on and which of its
/// points carry tarmac. A step that does not is inside a town, whose own
/// streets are drawn there, or on another road's trunk, which that road
/// draws.
#[derive(Clone, Copy)]
pub struct Line<'a> {
    pub points: &'a [DVec3],
    pub open: &'a [bool],
}

/// What a map is drawn OF: the planet with no sites in it (a town's
/// plateau and a road's corridor are drawn as the town and the road),
/// the sea's radius, the roads and the towns.
pub struct Scene<'a> {
    pub planet: &'a Planet,
    pub sea: f64,
    pub roads: &'a [Line<'a>],
    pub towns: &'a [Town],
}

/// A highway's own width, metres: its two lanes and the shoulders
/// either side (`road::ribbon`), so a road zoomed in on is as wide on
/// the map as it is on the ground.
const ROAD_M: f64 = 2.0 * (crate::road::ribbon::HALF + crate::road::ribbon::SHOULDER);
/// The narrowest a road is drawn, pixels, which is the page's own two:
/// a road truer than that is a road nobody can see.
const ROAD_PX: f64 = 2.0;
/// How far past the frame a road is still looked at, metres: longer
/// than any one piece of one (`road::PIECE`).
const MARGIN: f64 = 2_000.0;
/// How much of a lot a building is drawn over, so two buildings side by
/// side read as two when there are pixels enough to show it.
const BUILT: f64 = 0.9;
/// How big a LOT is on the picture, pixels, when a town is drawn wholly
/// as its built-up area, and when not at all: a lot under half a pixel is
/// a lace no eye can read, and past a pixel and a half the lots are the
/// picture.
const BUILT_UP_SOLID: f64 = 0.5;
const BUILT_UP_CLEAR: f64 = 1.5;

/// The map of a scene in a view, `width` by `height` pixels, on up to
/// `threads` threads: the ground, then the roads over it, then the towns
/// over those.
pub fn draw(scene: &Scene, view: &View, width: usize, height: usize, threads: usize) -> Picture {
    let heights = paint::heights(scene, view, width, height, threads);
    let mut pic = paint::ground(&heights, width, height, view.scale);
    draw_roads(&mut pic, scene, view);
    draw_towns(&mut pic, scene, view);
    pic
}

/// Every road in the view, on its tarmac only, at its true width or
/// `ROAD_PX`, whichever is wider.
fn draw_roads(pic: &mut Picture, scene: &Scene, view: &View) {
    let size = DVec2::new(pic.width as f64, pic.height as f64);
    // A road is looked at a little past the frame, so a piece that
    // crosses into it from outside is still drawn to the edge.
    let near = (view.reach(size) + MARGIN / view.radius).cos();
    let wide = (ROAD_M / view.scale).max(ROAD_PX);
    for line in scene.roads {
        let mut prev: Option<(DVec2, bool)> = None;
        for (k, p) in line.points.iter().enumerate() {
            let open = line.open.get(k).copied().unwrap_or(false);
            let here = (p.dot(view.centre) >= near)
                .then(|| view.to_px(*p))
                .flatten();
            if let (Some((a, was)), Some(b)) = (prev, here) {
                if was && open {
                    let (w, h) = (pic.width, pic.height);
                    pic.stroke(paint::at(a, w, h), paint::at(b, w, h), wide, paint::ROAD);
                }
            }
            prev = here.map(|b| (b, open));
        }
    }
}

/// Every town in the view: its streets and its squares, then its
/// buildings, each where the town's own plan puts it; and a dot for a
/// town too small at this scale to show any of that.
fn draw_towns(pic: &mut Picture, scene: &Scene, view: &View) {
    let size = DVec2::new(pic.width as f64, pic.height as f64);
    let reach = view.reach(size);
    for town in scene.towns {
        let out = town.radius * OUTLINE / view.radius;
        if town.dir.angle_between(view.centre) > reach + out {
            continue;
        }
        let tier = Tier::of(town.radius);
        let colour = paint::building(tier);
        // A town whose plan would be smaller than its own mark is drawn
        // AS the mark, the page's disc at its tier's size; past that it
        // is its streets and its buildings.
        if town.radius * OUTLINE / view.scale < paint::mark(tier) {
            if let Some(p) = view.to_px(town.dir) {
                pic.disc(
                    paint::at(p, pic.width, pic.height),
                    paint::mark(tier),
                    colour,
                );
            }
            continue;
        }
        draw_built_up(pic, town, view, colour, built_up(view.scale));
        let (w, h) = (pic.width, pic.height);
        let corner = |x: f64, z: f64| {
            let dir = (town.dir + town.east * (x / view.radius) + town.north * (z / view.radius))
                .normalize();
            view.to_px(dir).map(|p| paint::at(p, w, h))
        };
        let quad = |x: f64, z: f64, w: f64, d: f64| -> Option<[DVec2; 4]> {
            Some([
                corner(x - w / 2.0, z - d / 2.0)?,
                corner(x + w / 2.0, z - d / 2.0)?,
                corner(x + w / 2.0, z + d / 2.0)?,
                corner(x - w / 2.0, z + d / 2.0)?,
            ])
        };
        for piece in &town.pieces {
            let paving = if piece.square() {
                paint::SQUARE
            } else {
                paint::STREET
            };
            if let Some(q) = quad(piece.x, piece.z, piece.w, piece.d) {
                pic.fill(q, paving);
            }
        }
        for lot in &town.lots {
            let w = lot.w * BUILT;
            if let Some(q) = quad(lot.x, lot.z, w, w) {
                pic.fill(q, colour);
            }
        }
    }
}

/// How much of a town is laid down as its BUILT-UP AREA at a scale,
/// nought to one: all of it while a lot is under `BUILT_UP_SOLID` pixels
/// and none once it is past `BUILT_UP_CLEAR`, which is how a real map
/// generalises a city as it zooms out.
///
/// The map the owner asked about opens at forty eight metres a pixel,
/// where a lot is a fifth of a pixel and a street a sixth: laid on by
/// its own area the port's plan came out as a faint grey smudge twelve
/// pixels across in the middle of its plain, and the owner asked whether
/// a road grid like a city was supposed to be there. It was, and it was
/// under a pixel.
pub fn built_up(scale: f64) -> f64 {
    let lot = LOT / scale.max(1e-9);
    ((BUILT_UP_CLEAR - lot) / (BUILT_UP_CLEAR - BUILT_UP_SOLID)).clamp(0.0, 1.0)
}

/// The cells of a town's grid that carry a building, each a `PITCH` a
/// side on its block's own middle: the block and half the street round
/// it, so two built blocks side by side are one area with their street
/// in it and a lone one keeps its own frontage.
fn blocks_of(town: &Town) -> BTreeSet<(i64, i64)> {
    town.lots
        .iter()
        .map(|l| ((l.x / PITCH).round() as i64, (l.z / PITCH).round() as i64))
        .collect()
}

/// A town's built-up area, laid on by `alpha` in its own colour. Each
/// pixel asks where it stands in the TOWN's frame and whether that is a
/// built cell, four samples a pixel: the cells tile the ground, so there
/// is no seam where two of them share a pixel, which a quadrilateral a
/// cell laid on one after the other would leave.
fn draw_built_up(pic: &mut Picture, town: &Town, view: &View, colour: paint::Colour, alpha: f64) {
    if alpha <= 0.0 {
        return;
    }
    let Some(mid) = view.to_px(town.dir) else {
        return;
    };
    let cells = blocks_of(town);
    let reach = (town.radius * OUTLINE + PITCH) / view.scale;
    let (w, h) = (pic.width as f64, pic.height as f64);
    let at = paint::at(mid, pic.width, pic.height);
    let (x0, x1) = (
        (at.x - reach).floor().max(0.0),
        (at.x + reach).ceil().min(w - 1.0),
    );
    let (y0, y1) = (
        (at.y - reach).floor().max(0.0),
        (at.y + reach).ceil().min(h - 1.0),
    );
    if x0 > x1 || y0 > y1 {
        return;
    }
    let built = |col: f64, row: f64| {
        let dir = view.to_dir(DVec2::new(col - w * 0.5, h * 0.5 - row));
        let along = dir.dot(town.dir);
        if along <= 0.0 {
            return false;
        }
        let x = view.radius * dir.dot(town.east) / along;
        let z = view.radius * dir.dot(town.north) / along;
        cells.contains(&((x / PITCH).round() as i64, (z / PITCH).round() as i64))
    };
    for row in y0 as i64..=y1 as i64 {
        for col in x0 as i64..=x1 as i64 {
            let hits = [0.25, 0.75]
                .iter()
                .flat_map(|dy| [0.25, 0.75].map(|dx| (col as f64 + dx, row as f64 + dy)))
                .filter(|&(c, r)| built(c, r))
                .count();
            pic.blend(col, row, colour, alpha * hits as f64 / 4.0);
        }
    }
}

#[cfg(test)]
mod tests;
