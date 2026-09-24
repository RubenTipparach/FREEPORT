//! A DISTRICT of a town: the solid blocks of a square of tiles drawn as
//! ONE mesh while every tile in it is a block, and handed back to its
//! tiles the moment any one of them wants to be drawn as anything else.
//!
//! A tile is a block of the town's grid, which is the right size to
//! choose a grade by and the wrong size to draw from a kilometre off: the
//! port is two thousand of them, and past the farthest bake every one is
//! a mesh of a hundred triangles that costs the renderer an entity, a
//! visibility test in every view and a place in every batch, for a
//! picture a sixteenth of which would carry the same pixels. A district
//! is `SIDE` tiles a side and is that picture as one mesh.
//!
//! What is DRAWN changes and nothing else does: the tiles keep their own
//! blocks, their grades, their boxes and their lamps, and a district is
//! only ever a second way of drawing blocks that are all standing anyway.
//! It is switched in the same frame as the blocks under it, so no block
//! is missing for a frame and none is drawn twice.

use super::detail::TileState;
use super::tiles::{Tile, MASS};
use bevy::prelude::*;
use std::collections::BTreeMap;

/// How many tiles a side a district is: four, 194 m. A district is drawn
/// whole only while every tile in it is past the farthest bake, so the
/// districts straddling that ring are drawn as their tiles, and a bigger
/// district leaves more tiles drawn one at a time round it.
pub const SIDE: i64 = 4;

/// A district of a raised town: its tiles, the entity its blocks are
/// drawn as together, and whether that is what is drawn now.
pub struct District {
    pub tiles: Vec<usize>,
    pub entity: Option<Entity>,
    pub whole: bool,
}

/// A town's tiles gathered into districts, each the indices of its tiles,
/// in a fixed order.
pub fn districts_of(tiles: &[Tile]) -> Vec<Vec<usize>> {
    let mut out: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (k, t) in tiles.iter().enumerate() {
        let at = (t.key.0.div_euclid(SIDE), t.key.1.div_euclid(SIDE));
        out.entry(at).or_default().push(k);
    }
    out.into_values().collect()
}

/// The lots and pieces of street of a district, in its tiles' order.
pub fn parts_of(tiles: &[Tile], members: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let lots = members.iter().flat_map(|&k| tiles[k].lots.iter().copied());
    let pieces = members
        .iter()
        .flat_map(|&k| tiles[k].pieces.iter().copied());
    (lots.collect(), pieces.collect())
}

/// Whether a district can be drawn whole: every tile in it drawn as its
/// block and wanting to stay one.
pub fn whole(members: &[usize], state: &[TileState]) -> bool {
    members
        .iter()
        .all(|&k| state[k].shown.is_none() && state[k].grade == MASS)
}

/// Draw each district whole or as its tiles, whichever it can be now.
/// A tile drawn as something other than its block is left to itself,
/// because its own detail decides what its block is doing.
pub fn follow(commands: &mut Commands, districts: &mut [District], state: &[TileState]) -> usize {
    let mut drawn = 0;
    for d in districts.iter_mut() {
        let want = d.entity.is_some() && whole(&d.tiles, state);
        drawn += want as usize;
        if want == d.whole {
            continue;
        }
        d.whole = want;
        let (district, blocks) = if want {
            (Visibility::Inherited, Visibility::Hidden)
        } else {
            (Visibility::Hidden, Visibility::Inherited)
        };
        if let Some(e) = d.entity {
            commands.entity(e).insert(district);
        }
        for s in d.tiles.iter().map(|&k| &state[k]) {
            if let (None, Some(block)) = (&s.shown, s.mass) {
                commands.entity(block).insert(blocks);
            }
        }
    }
    drawn
}
