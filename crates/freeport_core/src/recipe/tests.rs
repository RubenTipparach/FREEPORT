//! The kit's tests: every shipped recipe compiles, a house is rooms and
//! walls and a lamp, a window that would meet a flight is dropped, a
//! tower's massing is a drum, and the distances are the kit's.

use super::*;

const HOUSE: &str = include_str!("../../../../assets/buildings/house.json");
const TOWER: &str = include_str!("../../../../assets/buildings/tower.json");
const COTTAGE: &str = include_str!("../../../../assets/buildings/cottage.json");
const HANGAR: &str = include_str!("../../../../assets/buildings/hangar.json");
const DOME: &str = include_str!("../../../../assets/buildings/dome.json");
const DUGOUT: &str = include_str!("../../../../assets/buildings/dugout.json");

fn ground(u: f64) -> Sample {
    Sample {
        d: -u,
        mat: TERRAIN,
        room: -1,
        curved: false,
    }
}

fn at(b: &Building, e: f64, n: f64, u: f64) -> Sample {
    b.sample(DVec3::new(e, n, u), ground(u))
}

#[test]
fn every_shipped_recipe_reads_and_compiles_without_a_warning() {
    for (text, storeys) in [
        (HOUSE, 3),
        (TOWER, 5),
        (COTTAGE, 2),
        (HANGAR, 1),
        (DOME, 1),
        (DUGOUT, 1),
    ] {
        let r = Recipe::parse(text).expect("a shipped recipe parses");
        let b = r.compile(storeys, 7);
        assert!(b.warnings.is_empty(), "{}: {:?}", r.name, b.warnings);
        assert!(!b.brushes.is_empty());
        assert!(
            b.hi.z > b.lo.z && b.hi.x - b.lo.x >= r.footprint[0],
            "{}",
            r.name
        );
    }
}

#[test]
fn a_house_has_a_room_a_storey_with_a_lamp_and_walls_of_concrete() {
    let r = Recipe::parse(HOUSE).expect("parses");
    assert_eq!((r.footprint, r.storeys, r.door), ([7.0, 6.0], [1, 3], 1.75));
    let b = r.compile(3, 7);
    assert_eq!(b.rooms, 3);
    assert_eq!(b.lamps.len(), 3);
    // The middle of the ground floor room is air, in room 0.
    let inside = at(&b, 0.0, 0.0, 1.5);
    assert!(inside.d < 0.0 && inside.room == 0, "{inside:?}");
    // The middle of the first floor is room 1.
    assert_eq!(at(&b, 0.0, 0.0, 4.5).room, 1);
    // The north wall is concrete, the corner pillar plate.
    let wall = at(&b, 0.0, 2.8, 1.5);
    assert!(wall.d > 0.0 && wall.mat == CONCRETE, "{wall:?}");
    let pillar = at(&b, 3.55, 3.05, 1.5);
    assert!(pillar.d > 0.0 && pillar.mat == PLATE, "{pillar:?}");
    // The doorway is air through the south wall.
    assert!(at(&b, 1.75, -2.9, 1.0).d < 0.0);
    // The lamp under the ceiling is a lamp.
    assert_eq!(at(&b, 0.0, 0.0, 2.9).mat, LAMP);
    // Under the plinth the ground is still the ground.
    assert_eq!(at(&b, 0.0, 0.0, -2.0).mat, TERRAIN);
}

#[test]
fn a_window_that_would_meet_the_flight_is_dropped_and_the_others_stay() {
    let r = Recipe::parse(HOUSE).expect("parses");
    // Three storeys, so two flights: the west wall's, then the east's.
    let b = r.compile(3, 7);
    assert!(b.dropped > 0, "no window met a flight");
    // The flight climbs the west wall on the ground floor, so the west
    // windows there are gone and the wall is whole; on the first floor
    // it climbs the east wall, so the east windows there are gone.
    let west0 = at(&b, -3.3, -1.5, 2.0);
    assert!(
        west0.d > 0.0,
        "a west window met the ground floor flight: {west0:?}"
    );
    let east1 = at(&b, 3.3, -1.5, 5.0);
    assert!(
        east1.d > 0.0,
        "an east window met the first floor flight: {east1:?}"
    );
    // The ground floor's east windows and the first floor's west are open.
    assert!(
        at(&b, 3.3, -1.5, 2.0).d < 0.0,
        "the east window is an opening"
    );
    assert!(
        at(&b, -3.3, -1.5, 5.0).d < 0.0,
        "the west window is an opening"
    );
    let windows = b.brushes.iter().filter(|br| br.window).count();
    assert!(windows >= 9, "{windows} windows left of 21");
    // A flight's own box is clear of every window's, grown.
    for f in b.brushes.iter().filter(|br| br.pitch.is_some()) {
        for w in b.brushes.iter().filter(|br| br.window) {
            assert!(!w.meets(
                f.lo - DVec3::splat(FLIGHT_CLEAR),
                f.hi + DVec3::splat(FLIGHT_CLEAR)
            ));
        }
    }
}

#[test]
fn a_tower_is_round_and_its_massing_is_a_drum() {
    let r = Recipe::parse(TOWER).expect("parses");
    let b = r.compile(4, 3);
    // Between the slit windows, which face north, east and west.
    let wall = at(&b, 1.9, 1.9, 1.5);
    assert!(wall.d > 0.0 && wall.curved, "{wall:?}");
    assert!(at(&b, 2.7, 0.0, 1.5).d < 0.0, "the east slit is an opening");
    assert!(at(&b, 0.0, 0.0, 1.5).d < 0.0, "the round room is air");
    // From afar on a metre lattice the room is not cut and the rail is
    // not there: solid concrete through the middle.
    let mass = b.massing(DVec3::new(0.0, 0.0, 1.5), ground(1.5), 1.0);
    assert!(mass.d > 0.0 && mass.mat == CONCRETE, "{mass:?}");
    let rail = b
        .brushes
        .iter()
        .find(|br| br.mat == PLATE && br.pitch.is_some())
        .expect("a rail");
    let on_rail = b.massing(rail.c, ground(rail.c.z), 1.0);
    assert!(on_rail.mat != PLATE, "a thin rail survives the massing");
}

#[test]
fn a_turned_box_is_bounded_by_its_corners_and_a_slab_is_one_brush() {
    // A box a metre long pitched to forty five degrees reaches
    // 0.707 up and along, not the metre of its diagonal.
    let pitched = oriented_extent(
        DVec3::new(0.5, 1.0, 0.1),
        None,
        None,
        Some((45f64.to_radians().cos(), 45f64.to_radians().sin())),
    );
    assert!((pitched.x - 0.5).abs() < 1e-12);
    assert!(
        (pitched.y - (1.0 + 0.1) * 0.5f64.sqrt()).abs() < 1e-9,
        "{pitched}"
    );
    assert!((pitched.z - (1.0 + 0.1) * 0.5f64.sqrt()).abs() < 1e-9);
    let slab = Building::slab(
        DVec3::new(0.0, 0.0, -0.05),
        DVec3::new(4.0, 3.5, 0.6),
        STREET,
        0.0,
    );
    assert_eq!(slab.brushes.len(), 1);
    let on = slab.sample(DVec3::new(1.0, 1.0, 0.2), ground(0.2));
    assert!(on.d > 0.0 && on.mat == STREET, "{on:?}");
    assert!(slab.sample(DVec3::new(1.0, 1.0, 0.5), ground(0.5)).d < 0.0);
}

#[test]
fn the_distances_are_the_kits() {
    assert_eq!(sd_box(DVec3::ZERO, DVec3::splat(1.0)), -1.0);
    assert!((sd_box(DVec3::new(2.0, 0.0, 0.0), DVec3::splat(1.0)) - 1.0).abs() < 1e-12);
    assert!((sd_box(DVec3::new(2.0, 2.0, 0.0), DVec3::splat(1.0)) - 2f64.sqrt()).abs() < 1e-12);
    assert_eq!(smax(1.0, -1.0, 0.5), 1.0);
    assert!(smax(0.0, 0.0, 0.5) > 0.0, "a fillet in the crease");
    let stairs = Brush {
        op: Op::Add,
        shape: Shape::Stairs,
        mat: CONCRETE,
        c: DVec3::ZERO,
        h: DVec3::new(0.5, 1.0, 0.5),
        rot: None,
        tilt: None,
        pitch: None,
        clip: None,
        blend: 0.0,
        room: -1,
        axis: Axis::Up,
        steps: 4,
        south: false,
        lo: DVec3::splat(-2.0),
        hi: DVec3::splat(2.0),
        window: false,
    };
    // The first step's tread is at a quarter of the rise, the last at the top.
    assert!(stairs.sd(DVec3::new(0.0, -0.75, -0.3)) < 0.0);
    assert!(stairs.sd(DVec3::new(0.0, -0.75, -0.2)) > 0.0);
    assert!(stairs.sd(DVec3::new(0.0, 0.75, 0.45)) < 0.0);
    assert_eq!(material("street"), Some(STREET));
    assert_eq!(material("glass"), Some(GLASS));
    assert_eq!(material("lit"), Some(LIT));
    assert_eq!(material("velvet"), None);
}
