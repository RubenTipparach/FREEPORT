//! A built town cut into TILES a block each, and what one is drawn and
//! walked into as.
//!
//! A town was ONE mesh at each of three detail levels, and which of the
//! three was drawn was decided for the whole town at once off the
//! distance to its edge. Standing in the port that is every one of its
//! fifteen hundred buildings at the nearest bake, 2.72 million triangles,
//! and raising it was 601 ms of one frame on the main thread: a city
//! three times the size would be twenty four million triangles and five
//! seconds. The owner's ask is whole cities of thousands of buildings,
//! with the far ones as solid blocks, and that is a question every BLOCK
//! answers for itself.
//!
//! So a town is its blocks. Every tile always has its MASSING, one box a
//! building and one quad a piece of street (`model::massing`,
//! `model::street_graded`), which is a few hundred triangles a block and
//! is what a city is drawn as from a kilometre. A tile nearer than that
//! is drawn from the library's own bakes at the grade its distance is
//! worth, built on a worker and swapped in when it is ready, so the
//! block it replaces is never missing for a frame. And a tile a body can
//! reach carries the boxes it is stopped by and the lamps that light it,
//! built the same way, because what stops a body is the same whatever it
//! is drawn as and nothing a kilometre off needs it.

use crate::buildings::Library;
use crate::terrain::{to_mesh_filtered, Vertex};
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::math::{DVec3, Vec3};
use bevy::prelude::*;
use freeport_core::field::{Block, GLASS};
use freeport_core::model::{self, street_graded, Part, STOREY};
use freeport_core::town::{lot_frame, Frame, Town, PITCH};
use std::collections::BTreeMap;

/// The grade a tile is drawn at past every bake: its buildings as solid
/// blocks and its streets as flat strips. The three before it are the
/// library's own three bakes.
pub const MASS: usize = 3;

/// How far a roof, a parapet or a plinth stands over the tallest storey a
/// lot is planned with, metres: what the plan's own bound is widened by
/// before a tile has been built and measured.
const ROOF: f64 = 4.0;
/// And how far under the ground at a lot's middle anything of it goes,
/// metres: a plinth on graded ground, down to under the lowest corner of
/// a twenty metre lot at the diagonal of the town's grade.
const FOOT: f64 = 2.0;

/// One block of a town: the lots and pieces of street in it, by their
/// places in the town's own lists, and roughly where it stands in the
/// town's own frame (east, north, up).
#[derive(Clone, Debug)]
pub struct Tile {
    /// Which block of the town's grid it is, in `PITCH`es east and north
    /// of the middle one.
    pub key: (i64, i64),
    pub lots: Vec<usize>,
    pub pieces: Vec<usize>,
    pub lo: DVec3,
    pub hi: DVec3,
}

impl Tile {
    /// How far a point in the town's own frame is from this tile, metres:
    /// nought inside it.
    pub fn distance(&self, p: DVec3) -> f64 {
        (p.clamp(self.lo, self.hi) - p).length()
    }
}

/// Which block a place on a town's grid belongs to.
///
/// A block's middle is a whole number of `PITCH`es from the town's, and
/// the street between two blocks runs on the line halfway between them,
/// so a piece of street on that line is on the boundary: the smallest
/// bias puts it in the block to its east or north every time, and a
/// crossing at a corner in the block to the north east of it.
fn key(x: f64, z: f64) -> (i64, i64) {
    let k = |v: f64| (v / PITCH + 0.5 + 1e-9).floor() as i64;
    (k(x), k(z))
}

/// A town cut into its tiles, in a fixed order.
pub fn tiles_of(town: &Town, radius: f64) -> Vec<Tile> {
    let middle = lot_frame(radius, town, 0.0, 0.0);
    let at = |x: f64, z: f64| middle.local(lot_frame(radius, town, x, z).world(DVec3::ZERO));
    let mut out: BTreeMap<(i64, i64), Tile> = BTreeMap::new();
    fn grow(
        out: &mut BTreeMap<(i64, i64), Tile>,
        k: (i64, i64),
        lo: DVec3,
        hi: DVec3,
    ) -> &mut Tile {
        let t = out.entry(k).or_insert(Tile {
            key: k,
            lots: Vec::new(),
            pieces: Vec::new(),
            lo,
            hi,
        });
        t.lo = t.lo.min(lo);
        t.hi = t.hi.max(hi);
        t
    }
    for (i, lot) in town.lots.iter().enumerate() {
        let c = at(lot.x, lot.z);
        let half = DVec3::new(lot.w, lot.w, 0.0) * 0.75;
        let high = lot.storeys as f64 * STOREY + ROOF;
        let (lo, hi) = (
            c - half - DVec3::Z * FOOT,
            c + half + DVec3::Z * (high + FOOT),
        );
        grow(&mut out, key(lot.x, lot.z), lo, hi).lots.push(i);
    }
    for (i, piece) in town.pieces.iter().enumerate() {
        let c = at(piece.x, piece.z);
        let half = DVec3::new(piece.w, piece.d, 0.0) * 0.5;
        let (lo, hi) = (c - half - DVec3::Z, c + half + DVec3::Z * ROOF);
        grow(&mut out, key(piece.x, piece.z), lo, hi).pieces.push(i);
    }
    out.into_values().collect()
}

/// Which grade a tile `distance` metres off is drawn at, from the grade
/// it is drawn at now: the three bakes out to `edges`, then the block.
///
/// With a margin either side of every edge, so a tile the eye is
/// standing on the line of is not rebuilt every frame it sways.
pub fn grade(current: usize, distance: f64, edges: [f64; 3], margin: f64) -> usize {
    let mut g = current.min(MASS);
    while g < MASS && distance > edges[g] * (1.0 + margin) {
        g += 1;
    }
    while g > 0 && distance < edges[g - 1] * (1.0 - margin) {
        g -= 1;
    }
    g
}

/// A tile DRAWN: its opaque triangles and its glazing as meshes ready to
/// hand to the renderer, and the bound that encloses both.
pub struct Drawn {
    pub opaque: Option<Mesh>,
    pub glass: Option<Mesh>,
    pub aabb: Option<Aabb>,
    pub triangles: usize,
}

/// A tile at a grade, built into meshes in the town's own frame. What a
/// worker does, so nothing here touches the world.
pub fn draw(
    library: &Library,
    town: &Town,
    tile: &Tile,
    grade: usize,
    radius: f64,
    sea: f64,
    seed: u32,
) -> Drawn {
    draw_part(
        library,
        town,
        (&tile.lots, &tile.pieces),
        grade,
        radius,
        sea,
        seed,
    )
}

/// Any set of a town's lots and pieces of street at a grade, as `draw`
/// builds one tile's: what a district's blocks are drawn with too.
pub fn draw_part(
    library: &Library,
    town: &Town,
    (lots, pieces): (&[usize], &[usize]),
    grade: usize,
    radius: f64,
    sea: f64,
    seed: u32,
) -> Drawn {
    let part = Part {
        lots,
        pieces,
        mesh: true,
        solids: false,
    };
    let fabric = model::fabric_part(
        town,
        radius,
        &part,
        |lot| {
            if grade >= MASS {
                library.massing(lot, seed)
            } else {
                library.model(lot, grade, seed)
            }
        },
        |piece| street_graded(piece, grade),
    );
    let frame = lot_frame(radius, town, 0.0, 0.0);
    let aabb = Aabb::enclosing(fabric.mesh.positions.iter().map(|&p| Vec3::from(p)));
    Drawn {
        opaque: mesh_of(&fabric.mesh, &frame, sea, false),
        glass: mesh_of(&fabric.mesh, &frame, sea, true),
        aabb,
        triangles: fabric.mesh.triangles(),
    }
}

/// The opaque half of a fabric's mesh or its glazing, placed for the
/// terrain shader, or nothing if there is none of it.
fn mesh_of(
    mesh: &freeport_core::dc::DcMesh,
    frame: &Frame,
    sea: f64,
    glazing: bool,
) -> Option<Mesh> {
    if !mesh.materials.iter().any(|&m| (m == GLASS) == glazing) {
        return None;
    }
    let place = |p: Vec3| Vertex::built(p, (frame.world(p.as_dvec3()).length() - sea) as f32);
    let mut out = to_mesh_filtered(mesh, place, |m| (m == GLASS) == glazing);
    // Vertex colour is the terrain shader's material and mapping payload,
    // not a tint for the standard material the glass is drawn with.
    if glazing {
        out.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    }
    // Bounds and collision live outside these buffers, so the vertex
    // data goes to the renderer without a CPU copy kept for every tile.
    out.asset_usage = RenderAssetUsages::RENDER_WORLD;
    Some(out)
}

/// What a tile STOPS a body with, and the lamps in it: in the world, off
/// the nearest bake, because a wall is the same wall however it is drawn.
pub struct Stops {
    pub blocks: Vec<Block>,
    pub lamps: Vec<(DVec3, f64)>,
    pub bounds: (DVec3, DVec3),
}

/// A tile's boxes and lamps, built. What a worker does.
pub fn stops(library: &Library, town: &Town, tile: &Tile, radius: f64, seed: u32) -> Stops {
    let part = Part {
        lots: &tile.lots,
        pieces: &tile.pieces,
        mesh: false,
        solids: true,
    };
    let fabric = model::fabric_part(
        town,
        radius,
        &part,
        |lot| library.solids(lot, seed),
        model::street,
    );
    let bounds =
        fabric
            .blocks
            .iter()
            .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), b| {
                let (blo, bhi) = b.bounds();
                (lo.min(blo), hi.max(bhi))
            });
    Stops {
        blocks: fabric.blocks,
        lamps: fabric.lamps,
        bounds,
    }
}

#[cfg(test)]
mod tests;
