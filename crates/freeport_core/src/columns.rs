//! The hex world as a DENSITY: a Goldberg column per tile, flat on top and
//! vertical at its sides, so the walker that walks the dual contoured world
//! walks this one with nothing added to it.
//!
//! The owner's ask was that walking respect the hexagons: "a rigid
//! collision system where I snap to the hex surface instead of some curved
//! interpolation". The smooth field is what `tiers.wgsl` DISPLACES a column
//! by, not what it draws: a column's top is flat at its middle's height and
//! its sides are vertical, so a walker on the smooth field floats over a
//! tile's low corner and sinks into its high one, and a wall between two
//! tiles is not there at all.
//!
//! What is here is that picture as a field, and nothing else changes: the
//! walker's ground, ceiling, wall and stand rules are the same functions
//! reading the same trait, because the picture is the collider is a rule
//! about the FIELD and not about the mesher. A column world is a field
//! whose surface happens to be flat in tiles.

use crate::field::{Density, Planet};
use crate::hex::{Grid, Tile};
use crate::stack::Stacks;
use glam::DVec3;

/// A planet drawn as hexagonal columns: which tiles, what the ground under
/// them is, and what has been built on them.
#[derive(Clone, Copy, Debug)]
pub struct Columns<'a> {
    pub grid: Grid,
    pub planet: &'a Planet,
    pub stacks: &'a Stacks,
}

impl<'a> Columns<'a> {
    /// The column world of `grid` on `planet`, with `stacks` built on it.
    pub fn new(grid: Grid, planet: &'a Planet, stacks: &'a Stacks) -> Columns<'a> {
        Columns {
            grid,
            planet,
            stacks,
        }
    }

    /// The radius of a tile's flat top, metres: the relief at the tile's
    /// own MIDDLE plus what has been built there, which is what
    /// `tiers.wgsl`'s `hex` entry point lifts every one of the tile's
    /// vertices by. The two are one number asked twice, so the collider is
    /// the picture rather than something near it, and a town's levelled
    /// site is in both, because `Planet::surface` applies the sites and
    /// `field.wgsl`'s transcription of it does too.
    pub fn top(&self, tile: Tile) -> f64 {
        self.planet.radius
            + self.planet.surface(self.grid.dir(tile)).0
            + self.stacks.at(self.grid, tile)
    }
}

impl Density for Columns<'_> {
    /// How far `p` is from the nearest face of the columns: positive in
    /// rock, negative in the air over one.
    ///
    /// Over a column it is the drop to its top, which is what makes a top
    /// FLAT and what the walker's `ground` bisects onto. Inside one it is
    /// the distance to the nearest way OUT, which is either straight up or
    /// sideways through a face whose neighbour's top is under this height,
    /// and that second case is the whole of what makes a step a wall: the
    /// walker pushes out along the gradient by the density over the slope,
    /// so a field that only ever measured the drop to the top would push a
    /// body pressed against a wall four centimetres a pass and leave it
    /// standing in the rock.
    ///
    /// A tile's boundary is taken as the perpendicular bisector of the two
    /// middles, which is the Voronoi edge; the mesh's own corner is the
    /// CENTROID of the three middles round it, and on a lattice this close
    /// to equilateral the centroid and the circumcentre are the same point
    /// to well under a tile.
    fn at(&self, p: DVec3) -> f64 {
        let r = p.length();
        if r <= f64::MIN_POSITIVE {
            return self.planet.radius;
        }
        let dir = p / r;
        let tile = self.grid.at(dir);
        let here = self.grid.dir(tile);
        let depth = self.top(tile) - r;
        if depth <= 0.0 {
            return depth;
        }
        // In rock. The way out is up unless a face is nearer, and a face
        // is only a way out where the column beyond it is lower than this
        // height. The cheap half of that test comes first: a neighbour
        // further off than the best way out so far cannot improve it, and
        // that is what keeps the tops, where the depth is nought, from
        // asking the field six more times a sample.
        let mut out = depth;
        for nb in self.grid.round(tile) {
            let across = here - self.grid.dir(nb);
            let len = across.length();
            if len <= f64::MIN_POSITIVE {
                continue;
            }
            let side = self.planet.radius * dir.dot(across / len).max(0.0);
            if side >= out {
                continue;
            }
            if self.top(nb) < r {
                out = side;
            }
        }
        out
    }

    /// What a column is made of, which is whatever was built on its tile:
    /// concrete on a wall, a street where a street was laid, and terrain
    /// everywhere nothing has said otherwise. The walker never asks, and
    /// the picture does (`tiers.wgsl` reads the same store).
    fn material(&self, p: DVec3) -> u8 {
        let dir = p.normalize_or(DVec3::Y);
        self.stacks.material(self.grid, self.grid.at(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::Stacks;
    use crate::walker::{Bounds, Input, Walker};

    /// A planetoid with tiles a metre across and the harness's own
    /// proportions of relief to radius, so the ground is as steep here as
    /// it is on the planet.
    /// Nothing built, which is what every test here starts from.
    fn bare() -> Stacks {
        Stacks::new()
    }

    fn gentle() -> (Grid, Planet) {
        let planet = Planet {
            radius: 2_000.0,
            relief: 16.0,
            lumps: 12.0,
            octaves: 9,
            overhang: 0.0,
            ledge: 0.0,
            seed: 7,
            sites: vec![],
        };
        (Grid::for_tile(planet.radius, 1.0), planet)
    }

    /// A planetoid steep enough that a tile's neighbour can stand a metre
    /// over it. The fractal's slope is about `4 * relief * octaves / (2 pi
    /// R / lumps)` whatever the tile size, so a WALL between two tiles is
    /// a property of the relief and not of the grid, and the harness's own
    /// planet has none (`sizes::the_steepest_step_between_two_tiles`).
    fn steep() -> (Grid, Planet) {
        let planet = Planet {
            relief: 400.0,
            ..gentle().1
        };
        (Grid::for_tile(planet.radius, 1.0), planet)
    }

    fn bounds(planet: &Planet) -> Bounds {
        let (floor, top) = planet.band();
        Bounds {
            radius: planet.radius,
            floor: floor - 2.0,
            top: top + 4.0,
            sea: 0.0,
        }
    }

    #[test]
    fn a_column_is_flat_on_top_and_the_field_is_the_drop_to_it() {
        let (grid, planet) = gentle();
        let bare = bare();
        let field = Columns::new(grid, &planet, &bare);
        let tile = grid.at(DVec3::new(0.3, 0.8, 0.5).normalize());
        let top = field.top(tile);
        // Every point of the tile's own hexagon has the SAME top, which is
        // what "flat" means and what the smooth field does not do.
        for corner in grid.corners(tile) {
            // A twentieth of the way to a corner, so the sample is still
            // this tile's.
            let inside = (corner * 0.05 + grid.dir(tile) * 0.95).normalize();
            assert_eq!(
                grid.at(inside),
                tile,
                "a point a twentieth of the way to a corner left the tile"
            );
            let over = field.at(inside * (top + 0.5));
            let under = field.at(inside * (top - 0.5));
            assert!((over + 0.5).abs() < 1e-6, "over the top: {over}");
            assert!(under > 0.0, "under the top: {under}");
        }
        // And the smooth field it is displaced from is NOT flat across the
        // tile, which is the whole reason this module exists.
        let middle = planet.surface(grid.dir(tile)).0;
        let spread = grid
            .corners(tile)
            .iter()
            .map(|c| (planet.surface(*c).0 - middle).abs())
            .fold(0.0_f64, f64::max);
        assert!(spread > 1e-4, "the smooth field is flat here too: {spread}");
    }

    #[test]
    fn a_neighbour_that_stands_higher_is_solid_where_the_lower_top_is() {
        let (grid, planet) = steep();
        let bare = bare();
        let field = Columns::new(grid, &planet, &bare);
        let start = grid.at(DVec3::new(0.2, 0.9, 0.4).normalize());
        let (low, high) = pair_with_a_step(&field, start).expect("a step over a metre");
        let stand = field.top(low);
        assert!(
            field.at(grid.dir(high) * (stand + 0.1)) > 0.0,
            "the higher column is not solid at the lower one's top"
        );
        assert!(
            field.at(grid.dir(low) * (stand + 0.1)) < 0.0,
            "the lower column is solid over its own top"
        );
    }

    #[test]
    fn a_body_pressed_against_a_step_is_pushed_out_sideways_in_one_pass() {
        // The reason the field measures the way OUT rather than the drop
        // to the top: a body a foot inside a wall has to come a foot out,
        // and a field that only knew the drop would push it four
        // centimetres (the walker's own gradient step) and leave it in the
        // rock.
        let (grid, planet) = steep();
        let bare = bare();
        let field = Columns::new(grid, &planet, &bare);
        let start = grid.at(DVec3::new(0.2, 0.9, 0.4).normalize());
        let (low, high) = pair_with_a_step(&field, start).expect("a step over a metre");
        // A third of a tile inside the high column, at the low one's own
        // standing height.
        let into = (grid.dir(high) * 0.7 + grid.dir(low) * 0.3).normalize();
        let stand = field.top(low) + 0.3;
        let out = field.at(into * stand);
        assert!(out > 0.0, "the sample is not in the rock: {out}");
        assert!(
            out < 0.5,
            "the way out is the drop to the top and not the wall: {out}"
        );
        let step = 0.04;
        let d = |axis: DVec3| {
            field.at(into * stand + axis * step) - field.at(into * stand - axis * step)
        };
        let grad = DVec3::new(d(DVec3::X), d(DVec3::Y), d(DVec3::Z));
        let sideways = grad - into * grad.dot(into);
        assert!(
            sideways.length() > grad.dot(into).abs(),
            "the gradient in a wall points down rather than out: {grad:?}"
        );
    }

    /// Two neighbouring tiles a metre or more apart in height, hunted out
    /// from `start` a ring at a time.
    fn pair_with_a_step(field: &Columns, start: Tile) -> Option<(Tile, Tile)> {
        let mut seen = vec![start];
        let mut edge = vec![start];
        for _ in 0..30 {
            let mut next = Vec::new();
            for t in edge.drain(..) {
                for nb in field.grid.round(t) {
                    if seen.contains(&nb) {
                        continue;
                    }
                    seen.push(nb);
                    next.push(nb);
                    if field.top(nb) - field.top(t) > 1.0 {
                        return Some((t, nb));
                    }
                }
            }
            edge = next;
        }
        None
    }

    #[test]
    fn the_walker_stands_on_a_column_rather_than_between_two() {
        let (grid, planet) = gentle();
        let bare = bare();
        let field = Columns::new(grid, &planet, &bare);
        let bounds = bounds(&planet);
        let dir = DVec3::new(0.1, 0.95, 0.3).normalize();
        let walker = Walker::enter(&field, &bounds, dir, DVec3::X);
        let tile = grid.at(walker.dir);
        assert!(
            (walker.foot - field.top(tile)).abs() < 0.02,
            "the feet are at {} and the tile's top is {}",
            walker.foot,
            field.top(tile)
        );
        assert!(walker.on_ground, "the walker did not land");
    }

    #[test]
    fn a_walk_over_the_columns_keeps_the_feet_on_a_tiles_own_top() {
        let (grid, planet) = gentle();
        let bare = bare();
        let field = Columns::new(grid, &planet, &bare);
        let bounds = bounds(&planet);
        let dir = DVec3::new(0.1, 0.95, 0.3).normalize();
        let mut walker = Walker::enter(&field, &bounds, dir, DVec3::X);
        let input = Input {
            forward: 1.0,
            ..Default::default()
        };
        let (mut worst, mut walked) = (0.0_f64, 0);
        for _ in 0..180 {
            walker.update(&field, &bounds, &input, 1.0 / 60.0);
            if !walker.on_ground {
                continue;
            }
            walked += 1;
            let tile = grid.at(walker.dir);
            worst = worst.max((walker.foot - field.top(tile)).abs());
        }
        assert!(walked > 150, "the walker was airborne for {walked} of 180");
        assert!(
            worst < 0.05,
            "the feet left the tile's own top by {worst:.3} m"
        );
    }

    #[test]
    fn a_tile_raised_under_the_feet_carries_the_walker_up_with_it() {
        let (grid, planet) = gentle();
        let bounds = bounds(&planet);
        let dir = DVec3::new(0.1, 0.95, 0.3).normalize();
        let bare = bare();
        let walker = Walker::enter(&Columns::new(grid, &planet, &bare), &bounds, dir, DVec3::X);
        let tile = grid.at(walker.dir);
        let mut built = Stacks::new();
        built.raise(grid, tile, 1.5);
        let field = Columns::new(grid, &planet, &built);
        assert!(
            (field.top(tile) - (walker.foot + 1.5)).abs() < 0.01,
            "the top went to {} and the feet were at {}",
            field.top(tile),
            walker.foot
        );
        // The same walker, one frame on the world as it is now: what is
        // built is walked the frame it lands, because the store the shader
        // reads IS the field the walker walks.
        let mut walker = walker;
        walker.update(&field, &bounds, &Input::default(), 1.0 / 60.0);
        assert!(
            (walker.foot - field.top(tile)).abs() < 0.02,
            "the feet are at {} and the raised top is {}",
            walker.foot,
            field.top(tile)
        );
    }

    #[test]
    fn a_patch_raised_a_metre_and_a_half_ahead_is_a_wall_the_walker_stops_at() {
        let (grid, planet) = gentle();
        let bounds = bounds(&planet);
        let dir = DVec3::new(0.1, 0.95, 0.3).normalize();
        let bare = bare();
        let mut walker = Walker::enter(&Columns::new(grid, &planet, &bare), &bounds, dir, DVec3::X);
        // A patch of nineteen tiles, its middle four metres along the way
        // the walker is facing, raised three clicks of the builder's own
        // half metre: well over the 0.6 m stride, so it is a wall and not
        // a step.
        let ahead = (walker.dir * planet.radius + walker.fwd * 4.0).normalize();
        let middle = grid.at(ahead);
        let mut built = Stacks::new();
        built.raise(grid, middle, 1.5);
        let mut ring = vec![middle];
        for _ in 0..2 {
            for t in ring.clone() {
                for nb in grid.round(t) {
                    if !ring.contains(&nb) {
                        ring.push(nb);
                        built.raise(grid, nb, 1.5);
                    }
                }
            }
        }
        assert_eq!(built.len(), 19, "the brush covered {} tiles", built.len());
        let field = Columns::new(grid, &planet, &built);
        let input = Input {
            forward: 1.0,
            ..Default::default()
        };
        let start = walker.dir;
        for _ in 0..240 {
            walker.update(&field, &bounds, &input, 1.0 / 60.0);
        }
        let went = start.angle_between(walker.dir) * planet.radius;
        let over = walker.foot - field.top(grid.at(walker.dir));
        // It is the patch's NEAR FACE that stops it and not its middle: a
        // brush of two rings reaches two and a half tiles out, so the wall
        // starts a metre and a half along and a body of 35 cm stops about
        // 1.15 m from where it set off. Measured: 1.11 m.
        assert!(
            (0.8..1.5).contains(&went),
            "the walker went {went:.2} m into a wall whose face is 1.5 m ahead"
        );
        assert_eq!(
            built.at(grid, grid.at(walker.dir)),
            0.0,
            "the walker climbed onto the patch, which is 1.5 m over a 0.6 m stride"
        );
        assert!(
            over.abs() < 0.05,
            "the walker did not climb, but its feet are {over:.2} m off the ground"
        );
    }
}

#[cfg(test)]
mod sizes {
    use super::*;
    use crate::walker::{Bounds, Input, Walker};
    use std::time::Instant;

    /// The harness's own planet, at the tile size it draws.
    fn harness() -> Planet {
        Planet {
            radius: 1_000_000.0,
            relief: 8_000.0,
            lumps: 12.0,
            octaves: 18,
            overhang: 3.0,
            ledge: 12.0,
            seed: 7,
            sites: vec![],
        }
    }

    /// What a frame of the walker costs on a column world, and what one
    /// sample of the field costs, on the harness's own planet. The walker
    /// asks the field a few hundred times a frame (a ring of twelve points
    /// at three heights, three passes, each with a gradient), so a sample
    /// that is cheap on a planetoid is a frame that is not on a planet.
    #[test]
    #[ignore = "a printer, not a check"]
    fn what_a_frame_of_the_walker_costs() {
        let planet = harness();
        let grid = Grid::for_tile(planet.radius, 1.0);
        let bare = Stacks::new();
        let field = Columns::new(grid, &planet, &bare);
        let (floor, top) = planet.band();
        let bounds = Bounds {
            radius: planet.radius,
            floor: floor - 2.0,
            top: top + 4.0,
            sea: 0.0,
        };
        let dir = DVec3::new(0.1, 0.95, 0.3).normalize();
        let clock = Instant::now();
        let mut walker = Walker::enter(&field, &bounds, dir, DVec3::X);
        println!("enter: {:.1} ms", clock.elapsed().as_secs_f64() * 1e3);
        let r = field.top(grid.at(dir));
        let clock = Instant::now();
        let mut sum = 0.0;
        for i in 0..10_000 {
            let a = i as f64 * 1e-6;
            sum += field.at(dir * (r - 0.3) + DVec3::X * a);
        }
        println!(
            "a sample: {:.1} us ({sum:.0})",
            clock.elapsed().as_secs_f64() * 1e6 / 10_000.0
        );
        let input = Input {
            forward: 1.0,
            ..Default::default()
        };
        let clock = Instant::now();
        for _ in 0..60 {
            walker.update(&field, &bounds, &input, 1.0 / 60.0);
        }
        println!(
            "a frame of walking: {:.1} ms",
            clock.elapsed().as_secs_f64() * 1e3 / 60.0
        );
    }

    /// How big a STEP the relief puts between two neighbouring tiles, on
    /// the harness's own planet and on a tenth of its tile size. A wall in
    /// a column world is a step over the walker's own stride (0.6 m), and
    /// whether there is one is a property of the RELIEF rather than of the
    /// grid: the fractal's slope is about `4 * relief * octaves / (2 pi R
    /// / lumps)` and a tile is what samples it.
    #[test]
    #[ignore = "a printer, not a check"]
    fn the_steepest_step_between_two_tiles() {
        let planet = Planet {
            radius: 1_000_000.0,
            relief: 8_000.0,
            lumps: 12.0,
            octaves: 18,
            overhang: 3.0,
            ledge: 12.0,
            seed: 7,
            sites: vec![],
        };
        for metres in [1.0, 0.5, 4.0] {
            let grid = Grid::for_tile(planet.radius, metres);
            let bare = Stacks::new();
            let field = Columns::new(grid, &planet, &bare);
            let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
            let (mut worst, mut total, mut n) = (0.0_f64, 0.0, 0u32);
            for i in 0..400 {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / 400.0;
                let s = (1.0 - y * y).sqrt();
                let a = golden * i as f64;
                let tile = field.grid.at(DVec3::new(s * a.cos(), y, s * a.sin()));
                let here = field.top(tile);
                for nb in field.grid.round(tile) {
                    let d = (field.top(nb) - here).abs();
                    worst = worst.max(d);
                    total += d;
                    n += 1;
                }
            }
            println!(
                "{metres:>4.1} m tiles ({} round the planet): {n} steps, mean {:.3} m, worst {worst:.3} m, the stride is 0.6",
                grid.count(),
                total / n as f64
            );
        }
    }
}
