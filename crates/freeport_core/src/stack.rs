//! What has been BUILT on the tiles: how much higher a column stands than
//! the relief under it, a tile at a time.
//!
//! A hex world is edited the way tenebris's is, by raising and lowering a
//! COLUMN rather than by cutting a shape out of a field: the ground is
//! already flat in tiles, so a tile is the unit a player builds in and a
//! block is what a step is worth. This is that store, and nothing else:
//! `columns::Columns` adds it to the relief so the walker walks what was
//! built the frame it lands, and the app hands the shader a window of it
//! so the picture is the same ground.
//!
//! Sorted pairs and a binary search rather than a `HashMap`, which is this
//! project's rule for anything replayed or hashed: a hash's order is not a
//! thing two clients agree on, and an edit list is exactly the thing they
//! have to.

use crate::hex::{Grid, Tile};

/// How much a tile has been raised, metres, for the tiles that have been.
#[derive(Clone, Debug, Default)]
pub struct Stacks {
    /// Sorted by key, so a lookup is a binary search and the order is the
    /// same on every machine that ever built this world.
    raised: Vec<(u64, f64)>,
}

/// One number naming a tile, which is what a sorted list needs. The
/// lattice is `n` tiles along an icosahedron edge, so `i` and `j` are
/// under `n + 1` and a face's own points fit in `(n + 1)^2`: at one metre
/// tiles on a thousand kilometre planet that is 1.2 million squared times
/// twenty, which is 2.9 * 10^13 and well inside a `u64`.
pub fn key(grid: Grid, tile: Tile) -> u64 {
    let n = grid.n as u64 + 1;
    (tile.face as u64) * n * n + (tile.i as u64) * n + tile.j as u64
}

/// The tile a key names, which is `key` undone: what a window is filled
/// from, since the store holds keys and a shader's window holds tiles.
/// None where the key names no tile of this grid.
pub fn tile(grid: Grid, key: u64) -> Option<Tile> {
    let n = grid.n as u64 + 1;
    let face = key / (n * n);
    let rest = key - face * n * n;
    let (i, j) = (rest / n, rest - (rest / n) * n);
    if face >= 20 || i + j > grid.n as u64 {
        return None;
    }
    grid.canonical(face as u8, i as i64, j as i64)
}

impl Stacks {
    /// Nothing built anywhere.
    pub fn new() -> Stacks {
        Stacks::default()
    }

    /// How far over the relief this tile's top stands, metres.
    pub fn at(&self, grid: Grid, tile: Tile) -> f64 {
        let k = key(grid, tile);
        match self.raised.binary_search_by_key(&k, |(k, _)| *k) {
            Ok(i) => self.raised[i].1,
            Err(_) => 0.0,
        }
    }

    /// Raise a tile by `metres`, or lower it with a negative one, and
    /// answer what it stands at now. A tile back at nought is REMOVED, so
    /// an edit taken back leaves the store as it was found and two worlds
    /// built to the same shape are the same list.
    pub fn raise(&mut self, grid: Grid, tile: Tile, metres: f64) -> f64 {
        let k = key(grid, tile);
        match self.raised.binary_search_by_key(&k, |(k, _)| *k) {
            Ok(i) => {
                let now = self.raised[i].1 + metres;
                if now.abs() < 1e-9 {
                    self.raised.remove(i);
                    0.0
                } else {
                    self.raised[i].1 = now;
                    now
                }
            }
            Err(i) => {
                if metres.abs() < 1e-9 {
                    return 0.0;
                }
                self.raised.insert(i, (k, metres));
                metres
            }
        }
    }

    /// How many tiles have been built on.
    pub fn len(&self) -> usize {
        self.raised.len()
    }

    /// Whether nothing has been built.
    pub fn is_empty(&self) -> bool {
        self.raised.is_empty()
    }

    /// Every tile built on, with what it stands at, in the store's own
    /// order: what an export writes and what a window is filled from.
    pub fn each(&self) -> impl Iterator<Item = (u64, f64)> + '_ {
        self.raised.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn grid() -> Grid {
        Grid::for_tile(2_000.0, 1.0)
    }

    #[test]
    fn a_tile_stands_where_it_was_raised_to_and_nowhere_else() {
        let grid = grid();
        let mut stacks = Stacks::new();
        let a = grid.at(DVec3::new(0.3, 0.8, 0.5).normalize());
        let b = grid.at(DVec3::new(-0.2, 0.9, 0.1).normalize());
        assert_eq!(stacks.at(grid, a), 0.0);
        assert_eq!(stacks.raise(grid, a, 0.5), 0.5);
        assert_eq!(stacks.raise(grid, a, 0.5), 1.0);
        assert_eq!(stacks.at(grid, a), 1.0);
        assert_eq!(stacks.at(grid, b), 0.0, "a neighbour was not raised");
        assert_eq!(stacks.len(), 1);
    }

    #[test]
    fn an_edit_taken_back_leaves_the_store_as_it_was_found() {
        let grid = grid();
        let mut stacks = Stacks::new();
        let a = grid.at(DVec3::new(0.3, 0.8, 0.5).normalize());
        stacks.raise(grid, a, 1.5);
        assert_eq!(stacks.len(), 1);
        stacks.raise(grid, a, -1.5);
        assert!(stacks.is_empty(), "a tile back at nought was kept");
        assert_eq!(stacks.at(grid, a), 0.0);
    }

    #[test]
    fn a_key_names_its_own_tile_and_nothing_else() {
        let grid = grid();
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        for i in 0..300 {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / 300.0;
            let s = (1.0 - y * y).sqrt();
            let a = golden * i as f64;
            let t = grid.at(DVec3::new(s * a.cos(), y, s * a.sin()));
            assert_eq!(tile(grid, key(grid, t)), Some(t), "{t:?} did not come back");
        }
    }

    #[test]
    fn the_keys_are_sorted_and_one_tile_is_one_key() {
        let grid = grid();
        let mut stacks = Stacks::new();
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        let mut tiles = Vec::new();
        for i in 0..200 {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / 200.0;
            let s = (1.0 - y * y).sqrt();
            let a = golden * i as f64;
            let tile = grid.at(DVec3::new(s * a.cos(), y, s * a.sin()));
            if tiles.contains(&tile) {
                continue;
            }
            tiles.push(tile);
            stacks.raise(grid, tile, 0.5);
        }
        assert_eq!(stacks.len(), tiles.len());
        let keys: Vec<u64> = stacks.each().map(|(k, _)| k).collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "the keys are not sorted"
        );
        for tile in tiles {
            assert_eq!(stacks.at(grid, tile), 0.5, "{tile:?} lost its height");
        }
    }
}
