//! A town's PLAN: which block of its grid carries what, and what stands
//! on each LOT of it.
//!
//! This is `docs/mockups/city-blocks.html` ported, which was drawn
//! against six real plans off OpenStreetMap and approved. A block is
//! `SIDE` lots a side and not one building: downtown and whatever faces
//! the market square fills its block with LARGE buildings of two by two
//! lots, each a corner on two streets; everywhere else the perimeter
//! lots build and the interior is a COURTYARD, because a lot with no
//! frontage is a lot nobody can get to. A suburb is the same ring with
//! space left in it. Measured on the page, that is 47% of the ground
//! under buildings against the 40 to 55% the real plans carry, where one
//! building a block was 27%.
//!
//! And a settlement has a TIER, which is the owner's own three: big
//! cities on the coast with towers downtown, small towns whose downtown
//! is one and two storey shops round a square, and villages of nothing
//! but houses dotted over the inland between them. The tier is read off
//! the settlement's own SIZE, and its size is how near the sea it stands
//! (`town::coastal`), so the three are one law rather than three.

use super::{
    demand, home_run, shape, streets_of, Lot, Piece, Zone, BLOCK, LOT, PITCH, SIDE, STREET, TOWN_AT,
};
use crate::field::hash3;
use crate::model::Kind;
use glam::DVec2;

/// Which SIDES of a block want a street: west, east, south and north,
/// and one more bit for a block that is the market SQUARE, which wants
/// no street through it.
pub(super) mod fronts {
    pub const WEST: u8 = 1;
    pub const EAST: u8 = 2;
    pub const SOUTH: u8 = 4;
    pub const NORTH: u8 = 8;
    pub const ALL: u8 = WEST | EAST | SOUTH | NORTH;
    pub const SQUARE: u8 = 16;
}

/// What KIND of place a settlement is, off its own size.
///
/// The thresholds are the mockup's own size table (`SIZES`), which
/// was read against the real plans: a market town from 180 m of nominal
/// radius and a city from 380. On the harness planet the biggest
/// settlement is 537 m and the size law's floor is 0.32 of that, so the
/// coastal cities are cities, the inland ones are towns, and the
/// villages a road grows (`town::WAYSIDE`) are villages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Houses and nothing else: no downtown, no square, one storey.
    Village,
    /// A downtown of one and two storey shops round a square.
    Town,
    /// Towers downtown, streets of two and three storey buildings, and
    /// suburbs.
    City,
}

/// Where a settlement stops being a village and where it becomes a
/// city, metres of nominal radius.
pub const TOWN_FROM: f64 = 180.0;
pub const CITY_FROM: f64 = 380.0;

impl Tier {
    /// The tier of a settlement of `radius`.
    pub fn of(radius: f64) -> Tier {
        if radius >= CITY_FROM {
            Tier::City
        } else if radius >= TOWN_FROM {
            Tier::Town
        } else {
            Tier::Village
        }
    }

    /// Its name, for a log.
    pub fn name(self) -> &'static str {
        match self {
            Tier::Village => "village",
            Tier::Town => "town",
            Tier::City => "city",
        }
    }
}

/// How many blocks a side the market SQUARE is, which is the mockup's
/// own ladder: none under 120 m of radius, one under 300, two under
/// 620 and three past that. A village has none, whatever its size.
pub fn square_of(tier: Tier, radius: f64) -> i64 {
    if tier == Tier::Village || radius < 120.0 {
        0
    } else if radius < 300.0 {
        1
    } else if radius < 620.0 {
        2
    } else {
        3
    }
}

/// How many of a suburb's LOTS carry a house at all. A suburb is a town
/// with SPACE in it, and what says so is the space rather than the
/// house: at one it is the same ring as downtown with shorter buildings
/// on it, which is what this was.
const SUBURB_FILL: f64 = 0.55;
/// How much of a town is a park, a plaza or a car park rather than a
/// block of buildings, on the block's own hash and thinned by the same
/// grain. A sixth of the ground is New York's own 16.1% of open space.
const OPEN: f64 = 0.16;
/// How far a suburban house WANTS to stand off the middle of its own
/// lot, metres either way, against a town house's own small jitter. A
/// setback and a garden are the other thing that says suburb.
///
/// What it GETS is whatever its own lot leaves once the building is on
/// it, which is `Kind::covers`: a lot is `LOT` across and the next lot
/// or the street's own inner kerb stands exactly `LOT / 2` from its
/// middle, so a building covering its whole lot may not move at all.
/// Every variant in the baked library is `LOT` square today, so this is
/// nought on the ground and is the number that comes back the day a
/// house is baked smaller.
const SUBURB_SETBACK: f64 = 4.5;
/// How far a town house stands off the middle of its own lot, metres
/// either way: enough that a terrace is not a ruler and no more, and
/// bounded by the same lot.
const TOWN_JITTER: f64 = 1.0;

/// A block's plan and its streets, which is what `lay` asks for.
pub(super) struct Plan {
    pub lots: Vec<Lot>,
    pub pieces: Vec<Piece>,
}

/// One block of a town's grid, once its zone is known: where it is,
/// what it is for and how tall.
struct Cell {
    i: i64,
    j: i64,
    zone: Zone,
    /// How far INTO the town proper this block stands, nought at its
    /// own edge and one at the middle, which is what the skyline is a
    /// function of.
    up: f64,
    /// Whether it fills itself with two by two buildings.
    big: bool,
}

/// Which block of a town's grid carries what, which sides of each block
/// want a street, and the lots on every block.
pub(super) fn plot(n: i64, radius: f64, along: DVec2, seed: u32) -> Plan {
    let wide = (2 * n + 1) as usize;
    let mut built = vec![0u8; wide * wide];
    let mut lots = Vec::new();
    let tier = Tier::of(radius);
    let square = square_of(tier, radius);
    let hash = |i: i64, j: i64, k: i64| hash3(i, j, k, seed);
    let in_square =
        |i: i64, j: i64| square > 0 && (0..square).contains(&i) && (0..square).contains(&j);
    for i in -n..=n {
        for j in -n..=n {
            let (cx, cz) = (i as f64 * PITCH, j as f64 * PITCH);
            let want = demand(cx, cz, radius, along, seed);
            let zone = Zone::of(want);
            if zone == Zone::Away {
                continue;
            }
            let at = (i + n) as usize * wide + (j + n) as usize;
            // The SQUARE is asked FIRST: a block of it carries nothing
            // and wants no street through it.
            if in_square(i, j) {
                built[at] |= fronts::SQUARE;
                continue;
            }
            // The blocks that FACE the square build whatever they are,
            // civic and commercial, and only the ones sharing an EDGE
            // with it: the ring with the diagonals in is twelve blocks,
            // which measured 12% of the ground against New York's 6.1%
            // of public and institutional land.
            let facing = square > 0
                && (((0..square).contains(&i) && (j == -1 || j == square))
                    || ((0..square).contains(&j) && (i == -1 || i == square)));
            // A park downtown, a field in the suburbs: the same hash,
            // read against the grain, so the holes in a town are the
            // shape of the holes in its own edge and there is ONE rule
            // about where a town is not rather than two. What faces
            // the square is never empty: a square with a field on one
            // side of it is not a square.
            let thin = shape::bite(cx, cz, radius, along, seed);
            if !facing && hash(i, j, 0) < OPEN + (1.0 - OPEN) * thin {
                continue;
            }
            // A village is houses and nothing else, whatever its demand
            // field says about its own middle.
            let zone = if tier == Tier::Village {
                Zone::Suburb
            } else {
                zone
            };
            // The raw demand is what it read, so moving `TOWN_AT` to
            // fix the MIX moved the SKYLINE with it. A zone's own share
            // of its own range is the number that means something here,
            // and the square law over it is the one this always had.
            let up = ((want - TOWN_AT) / (1.0 - TOWN_AT)).clamp(0.0, 1.0);
            let big = tier != Tier::Village && (zone == Zone::Core || facing);
            // A street on EVERY side of a built block, which is the
            // mockup's rule and what a block of a dozen houses has. A
            // suburb block fronted one side once, because a block was
            // ONE house then and a lone house in a ring of tarmac was a
            // moat; a ring of a dozen houses is a block.
            built[at] |= fronts::ALL;
            // And the road to it is paved all the way IN. A frontage on
            // its own is a driveway: it paves the one street beside the
            // block and stops, so a lone suburban house stood at an
            // isolated rectangle of tarmac that joined nothing.
            home_run(i, j, |k, m, side| {
                built[(k + n) as usize * wide + (m + n) as usize] |= side;
            });
            let cell = Cell {
                i,
                j,
                zone,
                up,
                big,
            };
            fill(&cell, tier, radius, along, seed, &mut lots);
        }
    }
    let pieces = streets_of(n, &built, square);
    Plan { lots, pieces }
}

/// The lots on one block: two by two buildings over the whole of it, or
/// the perimeter round a courtyard.
fn fill(cell: &Cell, tier: Tier, radius: f64, along: DVec2, seed: u32, lots: &mut Vec<Lot>) {
    let (cx, cz) = (cell.i as f64 * PITCH, cell.j as f64 * PITCH);
    let hash = |a: i64, b: i64, k: i64| hash3(cell.i * 31 + a, cell.j * 31 + b, k, seed);
    let side = SIDE as i64;
    // Where a lot's middle stands and which way its DOOR faces: the
    // street it fronts, which for a lot on the block's rim is the side it
    // stands on and for a two by two is the nearer of the block's two
    // long sides.
    let mut put = |a: i64, b: i64, w: f64, tall: u32, slot: u32| {
        let lot_w = w / LOT;
        let (x, z) = (
            cx + (a as f64 + lot_w * 0.5 - side as f64 * 0.5) * LOT,
            cz + (b as f64 + lot_w * 0.5 - side as f64 * 0.5) * LOT,
        );
        let yaw = if b == 0 {
            0.0
        } else if b as f64 + lot_w >= side as f64 {
            std::f64::consts::PI
        } else if a == 0 {
            -std::f64::consts::FRAC_PI_2
        } else {
            std::f64::consts::FRAC_PI_2
        };
        let (kind, storeys) = choose(tier, cell.zone, cell.big, tall, hash(a, b, 6));
        // A lot may move within its own LOT and no further, because the
        // next lot or the street's inner kerb is `LOT / 2` from its
        // middle and what stands past it is a wall in the road or in
        // the neighbour. The room is what the building does not cover,
        // and the zone says how much of that room it wants.
        let room = LOT * 0.5 * (1.0 - kind.covers()).max(0.0);
        let want = if cell.zone == Zone::Suburb {
            SUBURB_SETBACK
        } else {
            TOWN_JITTER
        };
        let jitter = 2.0 * want.min(room);
        lots.push(Lot {
            x: x + (hash(a, b, 4) - 0.5) * jitter,
            z: z + (hash(a, b, 5) - 0.5) * jitter,
            w,
            yaw,
            storeys,
            kind,
            id: ((cell.i + 64) as u32) << 13 | ((cell.j + 64) as u32) << 5 | slot,
        });
    };
    if cell.big {
        for a in 0..side / 2 {
            for b in 0..side / 2 {
                let tall = storeys_at(cell, hash(a, b, 3));
                put(a * 2, b * 2, 2.0 * LOT, tall, 16 + (a * 2 + b) as u32);
            }
        }
        return;
    }
    // Hollow: the perimeter builds, the interior is a court.
    for a in 0..side {
        for b in 0..side {
            let rim = a == 0 || b == 0 || a == side - 1 || b == side - 1;
            if !rim {
                continue;
            }
            if cell.zone == Zone::Suburb && hash(a, b, 7) > SUBURB_FILL {
                continue;
            }
            // And the GRAIN thins the ring lot by lot, on the lot's own
            // place, so a town's edge is ragged at the scale of the
            // thing that is actually laid and not only block by block.
            let (lx, lz) = (
                cx + (a as f64 + 0.5 - side as f64 * 0.5) * LOT,
                cz + (b as f64 + 0.5 - side as f64 * 0.5) * LOT,
            );
            if hash(a, b, 8) < shape::bite(lx, lz, radius, along, seed) {
                continue;
            }
            let tall = storeys_at(cell, hash(a, b, 3));
            put(a, b, LOT, tall, (a * side + b) as u32);
        }
    }
}

/// How tall a lot of a block WANTS to be: one storey in a suburb
/// whatever the hash says, because what makes a suburb a suburb is that
/// nothing on it is tall, and otherwise the square law over how far in
/// the block stands, with the lot's own dice on top.
fn storeys_at(cell: &Cell, dice: f64) -> u32 {
    match cell.zone {
        Zone::Suburb => 1,
        _ => 1 + ((dice * 0.4 + cell.up * cell.up) * 7.0).floor() as u32,
    }
}

/// What kind of building a lot of a wanted height gets, and how many
/// storeys it ends up with, by the settlement's tier and the block's
/// zone: towers in the middle of a city, shops in the middle of a town,
/// houses everywhere else, and the odd hangar among them.
fn choose(tier: Tier, zone: Zone, big: bool, tall: u32, pick: f64) -> (Kind, u32) {
    match (tier, big, zone) {
        (Tier::City, true, Zone::Core) => {
            if pick < 0.6 {
                (Kind::Block, tall.clamp(4, 8))
            } else {
                (Kind::Tower, tall.clamp(3, 6))
            }
        }
        // Facing a city's square: the civic and the commercial, which
        // are offices of a few storeys and not towers.
        (Tier::City, true, _) => (Kind::Block, tall.clamp(4, 6)),
        // A town's downtown is one and two storeys, which is the owner's
        // own number for it.
        (_, true, _) => (Kind::Shop, tall.clamp(1, 2)),
        (_, false, Zone::Suburb) => {
            if pick < 0.45 {
                (Kind::Bungalow, 1)
            } else if pick < 0.75 {
                (Kind::House, 1)
            } else {
                (Kind::Hangar, 1)
            }
        }
        (Tier::City, false, _) => {
            if pick < 0.7 {
                (Kind::House, tall.clamp(1, 2))
            } else {
                (Kind::Shop, tall.clamp(1, 3))
            }
        }
        (_, false, _) => {
            if pick < 0.6 {
                (Kind::House, tall.clamp(1, 2))
            } else {
                (Kind::Shop, tall.clamp(1, 2))
            }
        }
    }
}

/// The market SQUARE as one piece of paving over cells `0..square` each
/// way: between the streets round it, which the neighbouring blocks
/// laid, and with no street through it.
pub(super) fn square_piece(square: i64) -> Option<Piece> {
    if square <= 0 {
        return None;
    }
    let lo = -BLOCK * 0.5;
    let hi = square as f64 * PITCH - BLOCK * 0.5 - STREET;
    Some(Piece {
        x: (lo + hi) * 0.5,
        z: (lo + hi) * 0.5,
        w: hi - lo,
        d: hi - lo,
        arms: super::arm::SQUARE,
    })
}
