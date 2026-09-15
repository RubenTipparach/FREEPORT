//! A town GROWN on the tiles: what a building is on a column world.
//!
//! On the dual contoured world a building is a list of signed distance
//! brushes cut into the ground's own field (`recipe.rs`). A column world
//! has nothing to cut: a wall, a floor, a lintel and a step are all the
//! same thing there, a run of blocks in a column, which is tenebris's rule
//! and the hex mockup's. So a building is not PLACED on the tiles, it IS
//! the tiles: a lot's ring raised to its parapet and tagged concrete, its
//! inside left at the town's own level, and a gap in the ring on the
//! street side for the door. A street is a TAG and no rise at all, which
//! is the whole reason `stack::Stacks` holds a tile that stands at nought.
//!
//! What this does NOT do, and it is worth saying where the gap is: floors,
//! slabs and flights. A column has one top, so a storey over a storey is
//! not a thing a column can hold, and the hex mockup grew them by giving
//! every tile a list of RUNS rather than one height. That is the next
//! thing this module wants, and it is a change to the store rather than to
//! the growing.

use crate::field::{CONCRETE, STREET};
use crate::hex::{Grid, Tile};
use crate::stack::Stacks;
use crate::town::{Town, BLOCK};
use glam::DVec3;

/// How tall a storey stands, metres.
pub const STOREY: f64 = 3.0;
/// How thick a wall is, metres. A metre and a half is under two tiles at
/// the harness's own size, which is as thin as a wall can be and still be
/// a ring with an inside.
pub const WALL: f64 = 1.5;
/// How wide the doorway in the street wall is, metres.
pub const DOOR: f64 = 2.5;
/// How far over the top storey the parapet stands, metres.
pub const PARAPET: f64 = 0.5;

/// Every tile whose middle falls inside a rectangle of the town's frame,
/// `x` and `z` metres east and north of its middle, `w` by `d` metres.
///
/// Walked at half a tile, because a lattice of hexagons has no closed form
/// for the tiles inside a box and a walk at half the spacing cannot step
/// over one. The tiles come back distinct and in the order they were met.
pub fn tiles_in(grid: Grid, radius: f64, town: &Town, x: f64, z: f64, w: f64, d: f64) -> Vec<Tile> {
    let step = grid.spacing(radius) * 0.5;
    let (nx, nz) = (
        (w / step).ceil().max(1.0) as i64,
        (d / step).ceil().max(1.0) as i64,
    );
    let mut out = Vec::new();
    for i in 0..=nx {
        let e = x - w * 0.5 + w * i as f64 / nx as f64;
        for k in 0..=nz {
            let n = z - d * 0.5 + d * k as f64 / nz as f64;
            let dir = (town.dir + town.east * (e / radius) + town.north * (n / radius)).normalize();
            let tile = grid.at(dir);
            if !out.contains(&tile) {
                out.push(tile);
            }
        }
    }
    out
}

/// Where a tile's middle stands in the town's frame, metres east and north
/// of the town's own middle. The inverse of the map `town::lot_frame`
/// builds, to the small angle: a town is metres on a planet of kilometres,
/// so the tangent and the arc agree to a millionth over a lot.
pub fn where_in(radius: f64, town: &Town, dir: DVec3) -> (f64, f64) {
    (radius * dir.dot(town.east), radius * dir.dot(town.north))
}

/// A town's plan written into `stacks`: its streets tagged, its lots'
/// floors tagged, and its walls raised.
///
/// The order is the order a builder would use, and it matters: a street
/// laid first and a floor tagged over it leaves the lot concrete where the
/// two meet, which is what a kerb is. Nothing here raises the ground under
/// a town, because the town's SITE already levelled it (`Planet::sites`,
/// applied by `Planet::surface` and by `field.wgsl`'s transcription), so a
/// wall of three metres is three metres over a plateau and not over a
/// hill.
///
/// A tile is classified by where its own MIDDLE stands and never by which
/// sampled rectangles it turned up in: the first cut took the ring as the
/// footprint's tiles less the room's, and since a walk at half a tile puts
/// a tile that straddles the line in BOTH sets, every such tile was
/// dropped and the wall came out with holes in it wherever it was under
/// two tiles thick, which at `WALL` on this grid is everywhere.
pub fn grow(grid: Grid, radius: f64, town: &Town, stacks: &mut Stacks) {
    for piece in &town.pieces {
        for tile in tiles_in(grid, radius, town, piece.x, piece.z, piece.w, piece.d) {
            stacks.tag(grid, tile, STREET);
        }
    }
    for lot in &town.lots {
        let top = lot.storeys as f64 * STOREY + PARAPET;
        for tile in tiles_in(grid, radius, town, lot.x, lot.z, BLOCK, BLOCK) {
            let (e, n) = where_in(radius, town, grid.dir(tile));
            let (e, n) = (e - lot.x, n - lot.z);
            stacks.tag(grid, tile, CONCRETE);
            // The room is what the ring stands round, and the doorway is a
            // run of blocks in the street wall that was never laid rather
            // than a cut through a raised one, which is what a door is on
            // a column world.
            let room = e.abs() < BLOCK * 0.5 - WALL && n.abs() < BLOCK * 0.5 - WALL;
            let door = e.abs() < DOOR * 0.5 && n < -(BLOCK * 0.5 - WALL);
            if room || door {
                continue;
            }
            stacks.raise(grid, tile, top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{Planet, TERRAIN};
    use crate::town;

    /// A planetoid with a town on it, at the harness's own tile size.
    fn town_on_a_ball() -> (Grid, Planet, Town) {
        let planet = Planet {
            radius: 4_000.0,
            relief: 24.0,
            lumps: 12.0,
            octaves: 9,
            overhang: 0.0,
            ledge: 0.0,
            seed: 11,
            sites: vec![],
        };
        let towns = town::plan(&planet, planet.radius - 8.0, 40.0, 1, 11);
        assert!(!towns.is_empty(), "the ball grew no town to build on");
        let town = towns[0].clone();
        (Grid::for_tile(planet.radius, 1.0), planet, town)
    }

    /// Every tile a line of `n` steps from one point of the lot's frame to
    /// another crosses, which is what a walker going in at the door does.
    fn along(
        grid: Grid,
        radius: f64,
        town: &Town,
        from: (f64, f64),
        to: (f64, f64),
        n: usize,
    ) -> Vec<Tile> {
        let mut out = Vec::new();
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let e = from.0 + (to.0 - from.0) * t;
            let z = from.1 + (to.1 - from.1) * t;
            let dir = (town.dir + town.east * (e / radius) + town.north * (z / radius)).normalize();
            let tile = grid.at(dir);
            if !out.contains(&tile) {
                out.push(tile);
            }
        }
        out
    }

    #[test]
    fn a_lot_is_a_ring_of_walls_round_a_floor_with_a_door_in_it() {
        let (grid, planet, mut town) = town_on_a_ball();
        // One lot and no streets, so what is measured is the building.
        town.lots.truncate(1);
        town.pieces.clear();
        let lot = town.lots[0].clone();
        let mut stacks = Stacks::new();
        grow(grid, planet.radius, &town, &mut stacks);
        let top = lot.storeys as f64 * STOREY + PARAPET;
        let all = tiles_in(grid, planet.radius, &town, lot.x, lot.z, BLOCK, BLOCK);
        let (mut walls, mut floors) = (0, 0);
        for tile in &all {
            assert_eq!(stacks.material(grid, *tile), CONCRETE, "a lot tile is bare");
            let up = stacks.at(grid, *tile);
            if up == 0.0 {
                floors += 1;
            } else {
                assert_eq!(up, top, "a wall stands at the wrong height");
                walls += 1;
            }
        }
        assert!(walls > 0 && floors > 0, "{walls} walls and {floors} floors");
        // The door is a way IN: a walk from two metres outside the street
        // wall to the middle of the room crosses nothing raised.
        let door = along(
            grid,
            planet.radius,
            &town,
            (lot.x, lot.z - BLOCK * 0.5 - 2.0),
            (lot.x, lot.z),
            60,
        );
        for tile in &door {
            assert_eq!(
                stacks.at(grid, *tile),
                0.0,
                "the doorway is walled up at a tile of the way in"
            );
        }
        // And every other way in is not: a walk in from the east crosses
        // the ring.
        let side = along(
            grid,
            planet.radius,
            &town,
            (lot.x + BLOCK * 0.5 + 2.0, lot.z),
            (lot.x, lot.z),
            60,
        );
        assert!(
            side.iter().any(|t| stacks.at(grid, *t) == top),
            "the east wall has a hole in it"
        );
    }

    #[test]
    fn a_street_is_a_tag_and_no_rise_at_all() {
        let (grid, planet, mut town) = town_on_a_ball();
        town.lots.clear();
        assert!(!town.pieces.is_empty(), "the town has no streets");
        let mut stacks = Stacks::new();
        grow(grid, planet.radius, &town, &mut stacks);
        assert!(!stacks.is_empty(), "no street was laid");
        for (_, up, mat) in stacks.each() {
            assert_eq!(up, 0.0, "a street was raised");
            assert_eq!(mat, STREET);
        }
    }

    #[test]
    fn a_town_walks_as_it_draws_and_the_ground_round_it_is_untouched() {
        let (grid, mut planet, town) = town_on_a_ball();
        planet.sites = vec![town::site_of(&town)];
        let mut stacks = Stacks::new();
        grow(grid, planet.radius, &town, &mut stacks);
        let field = crate::columns::Columns::new(grid, &planet, &stacks);
        // A tile well outside the town carries nothing and stands on the
        // relief alone, which is what says the growing touched only what
        // the plan named.
        let (east, _) = town::frame_at(town.dir);
        let away = (town.dir + east * (400.0 / planet.radius)).normalize();
        let out = grid.at(away);
        assert_eq!(stacks.material(grid, out), TERRAIN);
        assert!(
            (field.top(out) - (planet.radius + planet.surface(grid.dir(out)).0)).abs() < 1e-9,
            "the ground outside the town moved"
        );
        // And a wall is a wall to the walker: wherever the store raised a
        // tile, the column's top stands exactly that far over the ground
        // the site already flattened.
        let mut walls = 0;
        for (key, up, _) in stacks.each() {
            if up == 0.0 {
                continue;
            }
            let Some(tile) = crate::stack::tile(grid, key) else {
                continue;
            };
            walls += 1;
            let ground = planet.radius + planet.surface(grid.dir(tile)).0;
            assert!(
                (field.top(tile) - (ground + up)).abs() < 1e-9,
                "a raised tile is not a wall to the walker"
            );
        }
        assert!(walls > 0, "the town grew no walls");
    }
}
