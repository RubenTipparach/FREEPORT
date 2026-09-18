/// A town's own PLAN, drawn: one character a block, so the outline,
/// the zones and where the streets run can be looked at.
///
/// A picture is the only check there is on a shape. The numbers say a
/// town has lots and streets and said exactly that when every town on
/// the body was the same circle.
fn drawn(town: &Town) -> String {
    let n = ((town.radius * (1.0 + super::REACH)) / PITCH).ceil() as i64;
    let mut out = String::new();
    for j in -n..=n {
        for i in -n..=n {
            let here = town.lots.iter().find(|l| {
                (l.x - i as f64 * PITCH).abs() <= PITCH * 0.5
                    && (l.z - j as f64 * PITCH).abs() <= PITCH * 0.5
            });
            out.push(match here {
                Some(l) if l.storeys >= 4 => '#',
                Some(l) if l.storeys >= 2 => '+',
                Some(_) => '.',
                None => ' ',
            });
        }
        out.push('\n');
    }
    out
}

/// The plan of a town at the harness's own scale, drawn, and the
/// sizes the rank size law gives a body of a hundred and sixty.
/// Ignored: a picture to look at rather than a rule.
#[test]
#[ignore]
fn draw_a_town_at_every_rank() {
    for index in [0usize, 1, 3, 12, 60, 159] {
        let radius = size_of(170.0, index, 7);
        let town = lay(DVec3::Y, 0.0, radius, index, 7);
        println!(
            "rank {index}: {radius:.0} m, {} lots, {} pieces of street",
            town.lots.len(),
            town.pieces.len()
        );
        print!("{}", drawn(&town));
    }
}

/// Towns are NOT all one size, and a town is not a circle. Both are
/// the owner's own ask and both were true of what this replaced: one
/// radius for every settlement on the body, and a disc of blocks cut
/// by `hypot(x, z) > radius`.
#[test]
fn towns_differ_in_size_and_are_not_discs() {
    let planet = planet();
    let sea = 996.0;
    let biggest = 40.0;
    let towns = plan(&planet, sea, biggest, 6, 7);
    assert!(towns.len() > 3, "{} towns", towns.len());
    // Zipf: the port is the biggest and they descend.
    let sizes: Vec<f64> = towns.iter().map(|t| t.radius).collect();
    println!(
        "{} towns of {:?} m",
        towns.len(),
        sizes.iter().map(|r| r.round()).collect::<Vec<_>>()
    );
    assert!(
        sizes[0] >= sizes[1..].iter().cloned().fold(0.0, f64::max),
        "the port is not the biggest town on the body"
    );
    assert!(
        sizes[0] > sizes[sizes.len() - 1] * 1.4,
        "the biggest town is {:.0} m and the smallest {:.0}, which is one size",
        sizes[0],
        sizes[sizes.len() - 1]
    );
    // And the outline is RAGGED: the furthest lot from the middle
    // stands well past the nearest edge of the town.
    let t = &towns[0];
    let mut by_bearing = [f64::MAX; 8];
    let mut out: f64 = 0.0;
    for l in &t.lots {
        let r = l.x.hypot(l.z);
        out = out.max(r);
        let a = l.z.atan2(l.x) + std::f64::consts::PI;
        let k = ((a / std::f64::consts::TAU * 8.0) as usize).min(7);
        by_bearing[k] = by_bearing[k].min(r);
    }
    // What says "not a disc" is that the town reaches FURTHER on one
    // bearing than another. Its longest reach alone says nothing: a
    // block grid quantises at `PITCH`, so a disc and a lobed outline
    // both end within one block of the nominal radius.
    let near = by_bearing.iter().cloned().fold(f64::MAX, f64::min);
    println!(
        "the port reaches {out:.0} m at its furthest and its nearest edge is {near:.0} m out, on a nominal {:.0}",
        t.radius
    );
    assert!(
        out > near * 1.5,
        "the town runs {out:.0} m one way and {near:.0} the other: a disc"
    );
}

/// A town is three ZONES: towers in the middle, streets of two and
/// three storey buildings round them, and one storey houses with
/// space between them on the outside. The owner asked for the suburbs
/// and this is what holds them.
#[test]
fn a_town_has_towers_in_the_middle_and_suburbs_outside() {
    let planet = planet();
    let towns = plan(&planet, 996.0, 60.0, 3, 7);
    let t = &towns[0];
    println!("the port, a block a character, # over four storeys, + two or three, . one:");
    print!("{}", drawn(t));
    let inner: Vec<&Lot> = t
        .lots
        .iter()
        .filter(|l| l.x.hypot(l.z) < t.radius * 0.35)
        .collect();
    let outer: Vec<&Lot> = t
        .lots
        .iter()
        .filter(|l| l.x.hypot(l.z) > t.radius * 0.8)
        .collect();
    assert!(
        !inner.is_empty() && !outer.is_empty(),
        "no middle or no edge"
    );
    let tall = |v: &[&Lot]| v.iter().map(|l| l.storeys as f64).sum::<f64>() / v.len() as f64;
    println!(
        "{} lots inside a third of the way out at {:.1} storeys, {} outside four fifths at {:.1}",
        inner.len(),
        tall(&inner),
        outer.len(),
        tall(&outer)
    );
    assert!(
        tall(&inner) > tall(&outer) * 1.8,
        "the middle is {:.1} storeys and the edge {:.1}: one skyline",
        tall(&inner),
        tall(&outer)
    );
    assert!(
        outer.iter().all(|l| l.storeys == 1),
        "something on the outside of the town is over one storey"
    );
    // And the suburb has SPACE in it: its blocks are further apart
    // than downtown's, because half of them carry nothing.
    let density = |v: &[&Lot], r0: f64, r1: f64| {
        v.len() as f64 / (std::f64::consts::PI * (r1 * r1 - r0 * r0))
    };
    let (mid, edge) = (
        density(&inner, 0.0, t.radius * 0.35),
        density(&outer, t.radius * 0.8, t.radius * (1.0 + super::REACH)),
    );
    println!("{mid:.4} lots a square metre in the middle, {edge:.4} on the edge");
    assert!(mid > edge * 1.5, "the suburb is as dense as downtown");
}

use super::*;

fn planet() -> Planet {
    Planet {
        radius: 1000.0,
        relief: 40.0,
        lumps: 6.0,
        octaves: 6,
        overhang: 1.0,
        ledge: 8.0,
        seed: 7,
        sites: vec![],
    }
}

#[test]
fn towns_stand_on_level_land_over_the_sea_and_apart() {
    let planet = planet();
    let sea = 996.0;
    let towns = plan(&planet, sea, 60.0, 4, 7);
    assert_eq!(towns.len(), 4, "four sites on a small planet");
    for (i, t) in towns.iter().enumerate() {
        assert_eq!(t.index, i);
        let h = planet.radius + t.h - sea;
        assert!((3.0..=40.0).contains(&h), "town {i} at {h} m over the sea");
        // A few lots rather than many: the smallest town on a body
        // is a VILLAGE now, and a floor written for one size is a
        // floor that fails on the tail of the rank size law.
        assert!(t.lots.len() >= 6, "{} lots", t.lots.len());
        assert!(t.pieces.len() > 40, "{} pieces of street", t.pieces.len());
        // Past its own nominal radius by the LOBES, which is what an
        // outline that is not a circle means, and no further.
        assert!(t
            .lots
            .iter()
            .all(|l| l.x.hypot(l.z) <= t.radius * (1.0 + REACH) + BLOCK));
        assert!(
            t.lots.iter().any(|l| l.storeys >= 4),
            "something tall in the middle"
        );
        assert!(
            t.lots.iter().any(|l| l.storeys == 1),
            "something low at the edge"
        );
        for u in towns.iter().skip(i + 1) {
            let apart = t.dir.angle_between(u.dir) * planet.radius;
            // BOTH radii and the country between them, because two
            // towns are not the same size any more.
            let want = t.radius + u.radius + BETWEEN;
            assert!(apart > want, "towns {apart:.0} m apart, wanting {want:.0}");
        }
    }
    // And the port, being the biggest, has the most in it.
    assert!(
        towns[0].lots.len() > towns[towns.len() - 1].lots.len(),
        "the port has {} lots and the smallest town {}",
        towns[0].lots.len(),
        towns[towns.len() - 1].lots.len()
    );
    // The PORT is the lowest, and only the port: the rest are taken
    // in a hashed order, or every town on the body stands on a shore
    // (`in_order`, and `biome::tests::towns_stand_inland_and_on_islands`).
    assert!(towns[1..].iter().all(|u| towns[0].h <= u.h));
    // A lot's frame is plumb where it stands and keeps the town's east.
    let t = &towns[0];
    let f = lot_frame(planet.radius, t, 30.0, -20.0);
    assert!((f.dir.length() - 1.0).abs() < 1e-12 && f.east.dot(f.dir).abs() < 1e-12);
    assert!(f.east.dot(t.east) > 0.99);
    let p = f.world(DVec3::new(1.0, 2.0, 3.0));
    assert!((f.local(p) - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
    assert!((f.local(f.dir * f.base)).length() < 1e-9);
    let site = site_of(t);
    assert_eq!(site.r, 2.0 * t.radius + APRON);
}

#[test]
fn a_levelled_site_flattens_the_ground_to_the_towns_height() {
    let mut planet = planet();
    let sea = 996.0;
    let towns = plan(&planet, sea, 40.0, 2, 7);
    let t = &towns[0];
    let before = ground_at(&planet, t.dir, 950.0, 1050.0);
    planet.sites.push(site_of(t));
    let mid = ground_at(&planet, t.dir, 950.0, 1050.0);
    assert!(
        (mid - (planet.radius + t.h)).abs() < 0.05,
        "the middle at {} against the level {}",
        mid - planet.radius,
        t.h
    );
    assert!(
        (before - mid).abs() < 5.0,
        "the level is near the ground that was there"
    );
    // Right across the town the ground is at the level; well outside it
    // the ground is its own.
    let f = lot_frame(planet.radius, t, t.radius * 0.8, 0.0);
    let edge = ground_at(&planet, f.dir, 950.0, 1050.0);
    assert!(
        (edge - (planet.radius + t.h)).abs() < 0.05,
        "the edge at {}",
        edge - planet.radius
    );
    let far = (t.dir + t.east * (t.radius * 4.0 / planet.radius)).normalize();
    let mut bare = planet.clone();
    bare.sites.clear();
    assert!(
        (ground_at(&planet, far, 950.0, 1050.0) - ground_at(&bare, far, 950.0, 1050.0)).abs()
            < 1e-9
    );
}
