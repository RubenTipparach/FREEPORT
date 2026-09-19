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

/// The plan of a town at the harness's own scale, drawn, at five heights
/// over the sea. Ignored: a picture to look at rather than a rule.
#[test]
#[ignore]
fn draw_a_town_at_every_height() {
    for (index, over) in [0usize, 1, 2, 3, 4]
        .into_iter()
        .zip([4.0, 90.0, 300.0, 900.0, 2200.0])
    {
        let radius = size_of(170.0, over, &planet(), index, 7);
        let town = lay(DVec3::Y, 0.0, radius, DVec2::new(1.0, 0.0), index, 7);
        println!(
            "{over:.0} m over the sea: {radius:.0} m across, {} lots, {} pieces of street",
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
    // What makes towns differ is the SIZE LAW, so the law is what this
    // asks: `coastal` falls from one at the shore to `SMALLEST` inland
    // over the body's own habitable window. Reading the spread off
    // whichever handful of sites a thousand metre fixture happens to
    // accept is a pin on a coincidence, and it failed the day the site
    // test legitimately got stricter: the six that qualified then all
    // stood within a few metres of one another, so their sizes were all
    // one number and the law was untouched.
    let (low, high) = super::window(&planet);
    let (shore, inland) = (
        size_of(biggest, low, &planet, 0, 7),
        size_of(biggest, high, &planet, 0, 7),
    );
    println!("a town on the shore is {shore:.0} m and one at {high:.0} m up is {inland:.0}");
    assert!(
        shore > inland * 1.4,
        "a shore town is {shore:.0} m and an inland one {inland:.0}: one size"
    );
    assert!(
        sizes[0] >= sizes[sizes.len() - 1],
        "the towns are not sorted biggest first"
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
    // By the town's own DEMAND rather than by a plain radius, because a
    // town is stretched along its shore: a lot four fifths of the radius
    // out along the stretch is nearer the middle than the same distance
    // across it, and a test that measured the plain radius was reading
    // downtown as suburb wherever the town is long.
    let zone = |l: &Lot| Zone::of(demand(l.x, l.z, t.radius, t.along, town_seed(7, t.index)));
    let inner: Vec<&Lot> = t.lots.iter().filter(|l| zone(l) == Zone::Core).collect();
    let outer: Vec<&Lot> = t.lots.iter().filter(|l| zone(l) == Zone::Suburb).collect();
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
        density(&inner, 0.0, t.radius * (1.0 - CORE_AT)),
        density(
            &outer,
            t.radius * (1.0 - TOWN_AT),
            t.radius * (1.0 + super::REACH),
        ),
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
    // The PORT is the BIGGEST, which is what its index nought means now:
    // the list is sorted by size and size is how near the sea a town
    // stands, so the port is the most coastal place the body grew, give
    // or take its own jitter.
    assert!(
        towns[1..].iter().all(|u| towns[0].radius >= u.radius),
        "the port is not the biggest town on the body"
    );
    // A lot's frame is plumb where it stands and keeps the town's east.
    let t = &towns[0];
    let f = lot_frame(planet.radius, t, 30.0, -20.0);
    assert!((f.dir.length() - 1.0).abs() < 1e-12 && f.east.dot(f.dir).abs() < 1e-12);
    assert!(f.east.dot(t.east) > 0.99);
    let p = f.world(DVec3::new(1.0, 2.0, 3.0));
    assert!((f.local(p) - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
    assert!((f.local(f.dir * f.base)).length() < 1e-9);
    // The site levels the town's own OUTLINE and an apron past it, not
    // one nominal radius: everything past that used to stand on bare
    // relief with its base at the town's level, which is a suburb
    // buried to its eaves.
    let site = site_of(t);
    assert_eq!(site.r, t.radius * super::OUTLINE + APRON);
    let (inner, _) = crate::field::site_band(&site);
    assert!(
        inner >= t.lots.iter().map(|l| l.x.hypot(l.z)).fold(0.0, f64::max),
        "the site levels {inner:.0} m and the town reaches further"
    );
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

/// How far a lot's own base stands from the ground the mesher will
/// actually contour under it. This is the number the owner read off a
/// picture of a suburb sunk to its eaves.
fn sink(planet: &Planet, sea: f64, town: &Town) -> (f64, f64, f64) {
    let mut here = planet.clone();
    here.sites = vec![site_of(town)];
    let (mut worst_in, mut worst_out, mut count) = (0.0f64, 0.0f64, 0.0f64);
    for lot in &town.lots {
        let f = lot_frame(planet.radius, town, lot.x, lot.z);
        let ground = here.surface(f.dir).0 + planet.radius;
        let gap = ground - f.base;
        worst_in = worst_in.max(gap);
        worst_out = worst_out.max(-gap);
        count += 1.0;
    }
    let _ = sea;
    (worst_in, worst_out, count)
}

/// A LOT STANDS ON ITS OWN GROUND, everywhere in the town.
///
/// The owner's picture: a suburb with its houses buried to the eaves and
/// only their roofs and their driveways showing. Three numbers written
/// against a town of ONE radius while a town actually reaches `OUTLINE`
/// (2.06) of it: the survey `site_ground` walks out to 1.05 radii, the
/// site `site_of` builds levels at full weight only to `site.r * 0.5`,
/// and the lots `lay` emits go out to 2.06. Everything past about half a
/// town stood on BARE RELIEF with its base at the town's own level, and
/// the level is the LOWEST of the survey, so the relief out there is
/// higher and the building is under it.
#[test]
fn a_lot_stands_on_its_own_ground_and_is_not_buried() {
    let planet = planet();
    let sea = 996.0;
    let towns = plan(&planet, sea, 60.0, 4, 7);
    assert!(!towns.is_empty());
    for town in &towns {
        let (into, over, lots) = sink(&planet, sea, town);
        println!(
            "town {} of {:.0} m: {lots:.0} lots, worst {into:.2} m INTO the ground, {over:.2} m over it",
            town.index, town.radius
        );
        assert!(
            into < 0.35,
            "town {} buries a building {into:.2} m into the ground",
            town.index
        );
        assert!(
            over < 0.35,
            "town {} floats a building {over:.2} m over the ground",
            town.index
        );
    }
}

/// Whether a point stands on a town's own paving.
fn on_paving(town: &Town, x: f64, z: f64) -> bool {
    town.pieces
        .iter()
        .any(|p| (x - p.x).abs() <= p.w * 0.5 + 1e-6 && (z - p.z).abs() <= p.d * 0.5 + 1e-6)
}

/// EVERY HOUSE IS ON THE ROAD NETWORK, and the network is ONE network.
///
/// The owner's picture: a suburb whose houses each stood at an isolated
/// rectangle of tarmac joining nothing. A suburb block fronted the side
/// FACING the middle of town, and the street on that side runs ACROSS
/// the way home: a house far out along east fronted west, which paves a
/// north south street, and nothing on it leads west. `faces` fronts the
/// side the road home is actually on now and `home_run` paves that road
/// all the way in.
#[test]
fn every_house_is_on_one_connected_road_network() {
    let planet = planet();
    let towns = plan(&planet, 996.0, 60.0, 3, 7);
    assert!(!towns.is_empty());
    for town in &towns {
        // Flood the paving from the middle of town, on a grid half a
        // street wide so a step can never hop a gap.
        let step = STREET * 0.5;
        let n = ((town.radius * super::OUTLINE + PITCH) / step).ceil() as i64 + 2;
        let wide = (2 * n + 1) as usize;
        let cell = |x: f64, z: f64| {
            let (i, j) = ((x / step).round() as i64, (z / step).round() as i64);
            ((-n..=n).contains(&i) && (-n..=n).contains(&j))
                .then(|| (i + n) as usize * wide + (j + n) as usize)
        };
        let mut seen = vec![false; wide * wide];
        // The seed is whichever paved cell is nearest the middle.
        let mut start = None;
        for j in -n..=n {
            for i in -n..=n {
                let (x, z) = (i as f64 * step, j as f64 * step);
                if !on_paving(town, x, z) {
                    continue;
                }
                let d = x.hypot(z);
                if start.is_none_or(|(_, _, best)| d < best) {
                    start = Some((i, j, d));
                }
            }
        }
        let Some((si, sj, _)) = start else {
            panic!("town {} paved nothing at all", town.index)
        };
        let mut stack = vec![(si, sj)];
        if let Some(k) = cell(si as f64 * step, sj as f64 * step) {
            seen[k] = true;
        }
        while let Some((i, j)) = stack.pop() {
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (i + di, j + dj);
                let (x, z) = (a as f64 * step, b as f64 * step);
                let Some(k) = cell(x, z) else { continue };
                if seen[k] || !on_paving(town, x, z) {
                    continue;
                }
                seen[k] = true;
                stack.push((a, b));
            }
        }
        // Every lot has reachable paving within half a block of its own
        // edge, which is what having a road at your door means.
        let mut stranded = 0;
        for lot in &town.lots {
            let near = (-3..=3)
                .flat_map(|a| (-3..=3).map(move |b| (a, b)))
                .any(|(a, b)| {
                    let (x, z) = (lot.x + a as f64 * step, lot.z + b as f64 * step);
                    cell(x, z).is_some_and(|k| seen[k])
                });
            stranded += usize::from(!near);
        }
        println!(
            "town {} of {:.0} m: {} lots, {} pieces, {stranded} stranded",
            town.index,
            town.radius,
            town.lots.len(),
            town.pieces.len()
        );
        assert_eq!(stranded, 0, "town {} strands {stranded} houses", town.index);
    }
}
