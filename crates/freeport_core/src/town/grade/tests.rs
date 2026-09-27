use super::*;

/// The harness body, whose relief is what a town has to be graded on.
fn harness() -> Planet {
    Planet {
        radius: 1_000_000.0,
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        seed: 7,
        sites: vec![].into(),
    }
}

/// A town's ground surveyed round `dir` out to `far` everywhere.
fn graded(dir: DVec3, far: f64) -> (Planet, Graded) {
    let p = harness();
    let g = Grade::survey(&p, dir, far, 25.0, f64::NEG_INFINITY, &|_| far);
    (p, g)
}

/// Level ground is its level everywhere and climbs nowhere.
#[test]
fn a_flat_grade_is_its_level_everywhere() {
    let g = Grade::flat(DVec3::Y, 12.5, 1e6);
    for (x, z) in [(0.0, 0.0), (400.0, -90.0), (-3_000.0, 7_000.0)] {
        assert_eq!(g.at(x, z), 12.5);
        assert_eq!(g.slope(x, z), DVec2::ZERO);
    }
    assert_eq!((g.low(), g.high()), (12.5, 12.5));
}

/// The graded ground climbs no faster than `GRADE` along either of the
/// town's own axes anywhere the town reads it, and its gradient is the
/// derivative of its own height.
#[test]
fn a_graded_ground_is_never_steeper_than_the_grade_along_its_axes() {
    let dir = DVec3::new(0.31, 0.62, -0.72).normalize();
    let (_, s) = graded(dir, 900.0);
    let g = &s.grade;
    let mut steepest = 0.0f64;
    for k in 0..2_000 {
        let a = k as f64 * 2.399_963;
        let r = 900.0 * ((k as f64 + 0.5) / 2_000.0).sqrt();
        let (x, z) = (r * a.cos(), r * a.sin());
        let sl = g.slope(x, z);
        steepest = steepest.max(sl.x.abs()).max(sl.y.abs());
        let e = 0.01;
        let dx = (g.at(x + e, z) - g.at(x - e, z)) / (2.0 * e);
        let dz = (g.at(x, z + e) - g.at(x, z - e)) / (2.0 * e);
        assert!((dx - sl.x).abs() < 1e-6 && (dz - sl.y).abs() < 1e-6);
        let h = g.at(x, z);
        assert!(h >= g.low() - 1e-9 && h <= g.high() + 1e-9);
    }
    assert!(
        steepest <= GRADE + 1e-9,
        "the ground climbs at {steepest:.4}"
    );
    assert!(
        steepest > GRADE * 0.2,
        "this body is not flat: {steepest:.4}"
    );
}

/// The ground is CUT and never filled: over the town it stands under the
/// natural ground, sampled finer than it was surveyed and off its grid,
/// and the survey's own deepest cut is what it says.
#[test]
fn a_graded_ground_only_ever_cuts() {
    let dir = DVec3::new(-0.44, 0.21, 0.87).normalize();
    let (p, s) = graded(dir, 700.0);
    let g = &s.grade;
    let mut deepest = 0.0f64;
    for j in -70..=70 {
        for i in -70..=70 {
            let (x, z) = (i as f64 * 9.7 + 0.3, j as f64 * 9.7 - 0.2);
            if x.hypot(z) > 690.0 {
                continue;
            }
            let bare = p.surface(g.dir_of(x, z)).0;
            let over = g.at(x, z) - bare;
            assert!(
                over < 0.05,
                "the ground stands {over:.2} m over the country"
            );
            deepest = deepest.max(-over);
        }
    }
    assert!(s.cut >= DIP && deepest > 0.0);
    assert!(
        (deepest - s.cut).abs() < s.cut * 0.25 + 1.0,
        "the survey says {:.1} m and the ground is {deepest:.1}",
        s.cut
    );
}

/// The highest the ground stands near a point bounds the ground there,
/// and is lower than the town's own highest point: which is what lets a
/// box over the low side of a town be ruled air.
#[test]
fn the_highest_ground_near_a_point_bounds_it() {
    let dir = DVec3::new(0.31, 0.62, -0.72).normalize();
    let (_, s) = graded(dir, 900.0);
    let g = &s.grade;
    let mut below = 0;
    for k in 0..400 {
        let a = k as f64 * 2.399_963;
        let r = 850.0 * ((k as f64 + 0.5) / 400.0).sqrt();
        let (x, z) = (r * a.cos(), r * a.sin());
        let top = g.high_near(g.dir_of(x, z), 30.0);
        for (dx, dz) in [(0.0, 0.0), (29.0, 0.0), (-20.0, 20.0), (0.0, -29.0)] {
            assert!(g.at(x + dx, z + dz) <= top + 1e-9);
        }
        below += usize::from(top < g.high() - 1.0);
    }
    assert!(
        below > 100,
        "the highest near a point is the town's highest everywhere"
    );
}

/// A direction and the point of the grid it is at are one place.
#[test]
fn a_direction_and_its_point_of_the_grid_are_one_place() {
    let dir = DVec3::new(0.1, -0.9, 0.3).normalize();
    let (_, s) = graded(dir, 500.0);
    let g = &s.grade;
    for (x, z) in [(0.0, 0.0), (123.4, -56.7), (-480.0, 300.0)] {
        let d = g.dir_of(x, z);
        assert!((g.at_dir(d) - g.at(x, z)).abs() < 1e-6);
    }
}

/// The harness body's port, planned and graded the way the game plans it.
fn port() -> (Planet, crate::town::Town) {
    let p = harness();
    let sea = p.radius + 1_100.0;
    let towns = crate::town::plan(&p, sea, 537.0, 1, 7);
    let t = towns
        .into_iter()
        .next()
        .expect("a port on the harness body");
    (p, t)
}

/// A frame leaning with the ground is still one frame: a point goes out
/// and comes back, and a box's turned up is the sheared ground's normal.
#[test]
fn a_leaning_frame_is_a_frame() {
    use crate::town::{frame_at, Frame};
    let (east, north) = frame_at(DVec3::Y);
    let f = Frame {
        dir: DVec3::Y,
        east,
        north,
        base: 1e6,
        lean: DVec2::new(0.06, -0.05),
    }
    .turned(0.7);
    for l in [DVec3::new(1.5, -2.0, 0.3), DVec3::new(-4.0, 0.0, 12.0)] {
        assert!((f.local(f.world(l)) - l).length() < 1e-6);
    }
    let up = f.axis(DVec3::Z);
    let normal = f.normal(DVec3::Z).normalize();
    assert!(
        up.angle_between(normal) < 1e-9,
        "a box's up is not the ground's"
    );
    // And the plane the frame shears onto is the ground's own slope.
    let (a, b) = (f.world(DVec3::ZERO), f.world(DVec3::new(10.0, 0.0, 0.0)));
    let rise = (b - a).dot(f.dir);
    assert!((rise - 10.0 * f.lean.x).abs() < 1e-6);
}

/// Every building of a graded town stands over ALL the ground under its
/// footprint, so no room has earth in it, on a plinth that reaches on
/// under the lowest of it, so no corner shows sky; and the town is not
/// flat, or this would prove nothing.
#[test]
fn a_building_on_graded_ground_stands_on_a_plinth() {
    let (p, t) = port();
    assert!(t.grade.is_some(), "a planned town is graded");
    let mut deepest = 0.0f64;
    for lot in &t.lots {
        let (frame, plinth) = crate::town::lot_stand(p.radius, &t, lot);
        let base = frame.base - p.radius;
        for k in 0..81 {
            let (u, v) = ((k % 9) as f64 / 8.0 - 0.5, (k / 9) as f64 / 8.0 - 0.5);
            let g = t.ground(lot.x + u * lot.w, lot.z + v * lot.w);
            assert!(g <= base + 0.05, "earth {:.2} m up a room", g - base);
            assert!(base - plinth <= g - 0.29, "a plinth short of its ground");
        }
        deepest = deepest.max(plinth);
    }
    println!("the port's deepest plinth is {deepest:.2} m");
    assert!(deepest > 0.5, "the port stands on flat ground");
}

/// Every piece of street is LAID on its ground: each point of it stands
/// over the ground at its own place by exactly the height its own model
/// gives it, rather than level at its middle's height with one end
/// buried and the other floating. The market square too, which is one
/// piece 137 m across in a city.
#[test]
fn a_street_is_laid_on_its_graded_ground() {
    use crate::model::{fabric_part, street, Part};
    let (p, t) = port();
    let g = t.grade.as_ref().expect("a planned town is graded");
    let middle = crate::town::lot_frame(p.radius, &t, 0.0, 0.0);
    let (mut worst, mut widest) = (0.0f64, 0.0f64);
    for k in 0..t.pieces.len() {
        let piece = &t.pieces[k];
        let own = street(piece);
        let part = Part {
            lots: &[],
            pieces: &[k],
            mesh: true,
            solids: false,
        };
        let laid = fabric_part(&t, p.radius, &part, |_| unreachable!(), street);
        for (a, b) in own.mesh.positions.iter().zip(&laid.mesh.positions) {
            let w = middle.world(DVec3::new(b[0] as f64, b[1] as f64, b[2] as f64));
            let over = w.length() - p.radius - g.at_dir(w.normalize());
            worst = worst.max((over - a[2] as f64).abs());
        }
        widest = widest.max(piece.w.max(piece.d));
    }
    println!("a street stands {worst:.4} m off where it belongs at worst; the widest piece is {widest:.0} m");
    assert!(worst < 0.01, "a street {worst:.3} m off its ground");
}
