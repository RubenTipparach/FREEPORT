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
use crate::town::{frame_at, Tier, Town, OUTLINE};
use glam::{DVec2, DVec3};

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

#[cfg(test)]
mod tests;
