/// A town's own PLAN, drawn: one character a block, so the outline,
/// the zones and where the streets run can be looked at.
///
/// A picture is the only check there is on a shape. The numbers say a
/// town has lots and streets and said exactly that when every town on
/// the body was the same circle.
pub(super) fn drawn(town: &Town) -> String {
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

/// The plan of a town at the harness's own scale, drawn, at five
/// distances from the sea. Ignored: a picture to look at rather than a
/// rule.
#[test]
#[ignore]
fn draw_a_town_at_every_height() {
    for (index, shore) in [0usize, 1, 2, 3, 4]
        .into_iter()
        .zip([0.0, 10.0, 40.0, 100.0, 300.0])
    {
        let radius = size_of(170.0, shore, &planet(), index, 7);
        let town = lay(DVec3::Y, 0.0, radius, DVec2::new(1.0, 0.0), index, 7);
        println!(
            "{shore:.0} m from the sea: {radius:.0} m across, {} lots, {} pieces of street",
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
    // Three hundred metres, because the fixture ball's level ground
    // stands a hundred metres or more from its own sea, which is three
    // times the size law's reach: the towns it grows are all at the
    // law's floor, a third of this, and a port under fifty metres is
    // one block of four by four with nothing to be ragged with.
    let biggest = 300.0;
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
    // over `COAST` of the body's own radius. Reading the spread off
    // whichever handful of sites a thousand metre fixture happens to
    // accept is a pin on a coincidence, and it failed the day the site
    // test legitimately got stricter: the six that qualified then all
    // stood within a few metres of one another, so their sizes were all
    // one number and the law was untouched.
    let far = planet.radius * 0.5;
    let (shore, inland) = (
        size_of(biggest, 0.0, &planet, 0, 7),
        size_of(biggest, far, &planet, 0, 7),
    );
    println!("a town on the shore is {shore:.0} m and one {far:.0} m inland is {inland:.0}");
    // And the towns the body actually got are sized that way: the
    // biggest stands nearer the water than the smallest does.
    let sea_at = Shore::of(&planet, sea);
    let (first, last) = (&towns[0], &towns[towns.len() - 1]);
    println!(
        "the port is {:.0} m from the sea and the smallest town {:.0}",
        sea_at.distance(first.dir),
        sea_at.distance(last.dir)
    );
    assert!(
        sea_at.distance(first.dir) <= sea_at.distance(last.dir),
        "the biggest town stands further from the sea than the smallest"
    );
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
    // A town big enough to HAVE a downtown. At 60 m, which is what
    // this asked for, the core is `CORE_AT` of a demand disc a hundred
    // and twenty metres across and holds no whole block at all: a
    // hamlet has no towers in it, which is right and is not what this
    // test is about.
    // A CITY, laid rather than planned: the fixture ball has no plain
    // for one and this test is about the plan, not the site.
    let _ = planet;
    let towns = [lay(DVec3::Y, 0.0, 537.0, DVec2::new(1.0, 0.0), 0, 7)];
    let t = &towns[0];
    println!("the port, a block a character, # over four storeys, + two or three, . one:");
    print!("{}", drawn(t));
    // By the town's own DEMAND rather than by a plain radius, because a
    // town is stretched along its shore: a lot four fifths of the radius
    // out along the stretch is nearer the middle than the same distance
    // across it, and a test that measured the plain radius was reading
    // downtown as suburb wherever the town is long.
    // At the lot's own BLOCK, which is what `plot` asked: a lot is
    // jittered off its block's middle by up to half a pitch, so asking
    // the demand where the building ended up reads a lot near a zone
    // boundary on the wrong side of it.
    let block = |v: f64| (v / PITCH).round() * PITCH;
    let zone = |l: &Lot| Zone::of(demand(block(l.x), block(l.z), t.radius, t.along, t.seed));
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
        sites: vec![].into(),
    }
}

#[test]
fn towns_stand_on_level_land_over_the_sea_and_apart() {
    let planet = planet();
    let sea = 996.0;
    let towns = plan(&planet, sea, 250.0, 4, 7);
    assert_eq!(towns.len(), 4, "four sites on a small planet");
    for (i, t) in towns.iter().enumerate() {
        assert_eq!(t.index, i);
        let h = planet.radius + t.h - sea;
        assert!((3.0..=40.0).contains(&h), "town {i} at {h} m over the sea");
        // A few lots rather than many: the smallest town on a body
        // is a VILLAGE now, and a floor written for one size is a
        // floor that fails on the tail of the rank size law.
        // FOUR and not six, and the reason is the mix rather than the
        // plan: three quarters of a town is suburb now, and
        // `SUBURB_FILL` leaves nearly half a suburb's blocks empty, so
        // a 60 m hamlet carries fewer buildings on exactly the same
        // ground. What this is guarding is a town with nothing on it.
        assert!(t.lots.len() >= 4, "{} lots", t.lots.len());
        // And the streets follow the lots, so the same mix takes the
        // paving down with it: 36 on this hamlet against the 40 this
        // asked for. What it guards is a town with no streets.
        assert!(t.pieces.len() > 20, "{} pieces of street", t.pieces.len());
        // Past its own nominal radius by the LOBES, which is what an
        // outline that is not a circle means, and no further.
        assert!(t
            .lots
            .iter()
            .all(|l| l.x.hypot(l.z) <= t.radius * (1.0 + REACH) + BLOCK));
        // TALLER THAN A HOUSE, and not a tower: these are 60 m towns,
        // which is a hamlet, and a hamlet's own middle is two and
        // three storey buildings rather than offices. What says a town
        // has a downtown is its SIZE, and
        // `a_town_is_a_quarter_towers_and_three_quarters_houses` is
        // where that is held.
        // A VILLAGE is one storey everywhere, which is what the fixture
        // ball's towns are; anything bigger has a downtown.
        if Tier::of(t.radius) == Tier::Village {
            assert!(
                t.lots.iter().all(|l| l.storeys == 1),
                "a village with something over one storey in it"
            );
        } else {
            assert!(
                t.lots.iter().any(|l| l.storeys >= 2),
                "nothing over one storey anywhere in the middle"
            );
        }
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
    let towns = plan(&planet, sea, 250.0, 2, 7);
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
    // The ground is CUT and never filled, and no deeper than the site
    // was allowed.
    assert!(
        before - mid >= -0.01 && before - mid <= CUT + 0.01,
        "the ground is {:.2} m under the ground that was there, past the {CUT} a site may cut",
        before - mid
    );
    // Right across the town the ground is the town's own; well outside
    // it the ground is its own.
    let f = lot_frame(planet.radius, t, t.radius * 0.8, 0.0);
    let edge = ground_at(&planet, f.dir, 950.0, 1050.0);
    assert!(
        (edge - f.base).abs() < 0.05,
        "the edge at {} against the town's ground {}",
        edge - planet.radius,
        f.base - planet.radius
    );
    // Past the outline, the apron AND the skirt the blend ramps over,
    // which is where the site stops saying anything at all.
    let site = site_of(t);
    let out = site.r + crate::field::site_skirt(&site) + 10.0;
    let far = (t.dir + t.east * (out / planet.radius)).normalize();
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
    here.sites = vec![site_of(town)].into();
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
    let towns = plan(&planet, 996.0, 250.0, 3, 7);
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

/// A round site is an ARC OF NO LENGTH, which is what lets one type
/// serve a town's disc and a road's corridor.
#[test]
fn a_round_site_is_an_arc_of_no_length() {
    let site = Site::round(DVec3::new(0.3, 0.4, 0.87).normalize(), 42.0, 80.0);
    for probe in [DVec3::X, DVec3::Y, DVec3::Z, site.dir, -site.dir] {
        assert_eq!(site.along(probe), 0.0, "a disc has nowhere to be along");
        let (at, h) = site.nearest(probe);
        assert_eq!(at, site.dir);
        assert_eq!(h, 42.0);
    }
    assert_eq!(site.reach(), 0.0);
    assert_eq!(site.grade(1_000_000.0), 0.0);
}

/// An arc's nearest point is ON it, its ends CLAMP, and its level ramps
/// from one to the other: what a corridor cut along a road is made of.
#[test]
fn an_arcs_nearest_point_is_on_it_and_its_level_ramps() {
    let radius = 1_000_000.0;
    let a = DVec3::new(0.0, 0.1, 1.0).normalize();
    // A kilometre along, which is a road's own piece at this scale.
    let b = {
        let (east, _) = frame_at(a);
        (a + east * (4_000.0 / radius)).normalize()
    };
    let site = Site::arc((a, 100.0), (b, 140.0), 12.0);
    assert!(
        (site.along(a) - 0.0).abs() < 1e-9,
        "the start is nought along"
    );
    assert!((site.along(b) - 1.0).abs() < 1e-9, "the end is one along");
    // The middle of the arc is half along and half way up the ramp.
    let mid = (a + b).normalize();
    assert!((site.along(mid) - 0.5).abs() < 1e-6, "{}", site.along(mid));
    let (at, h) = site.nearest(mid);
    assert!(at.distance(mid) < 1e-9, "the middle is already on the arc");
    assert!((h - 120.0).abs() < 1e-6, "the level ramps: {h}");
    // Off to one side, the nearest point is still ON the arc and the
    // level is the one at that point rather than at either end.
    let (_, north) = frame_at(mid);
    let off = (mid + north * (30.0 / radius)).normalize();
    let (at, h) = site.nearest(off);
    let pole = a.cross(b).normalize();
    assert!(
        at.dot(pole).abs() < 1e-9,
        "the nearest point is on the circle"
    );
    assert!((h - 120.0).abs() < 0.5, "abreast of the middle: {h}");
    // And past either end it CLAMPS, so a corridor does not reach round
    // the planet.
    let (east, _) = frame_at(a);
    let behind = (a - east * (9_000.0 / radius)).normalize();
    assert_eq!(site.along(behind), 0.0);
    assert_eq!(site.nearest(behind), (a, 100.0));
    let beyond = (b + east * (9_000.0 / radius)).normalize();
    assert_eq!(site.along(beyond), 1.0);
    assert_eq!(site.nearest(beyond), (b, 140.0));
    // Its grade is what it climbs: 40 m over 4 km.
    assert!(
        (site.grade(radius) - 0.01).abs() < 1e-4,
        "{}",
        site.grade(radius)
    );
}

/// A town's own outline never moves faster than the bound its skirt is
/// widened by, which is what keeps the planet's slope bound where it was.
///
/// The bound is what `field::site_skirt` reads, so a town whose edge
/// swung faster than this would fade to the relief over a band steeper
/// than `Planet::steepest` allows, and a chunk with surface in it would
/// be ruled empty: a hole in the world. Measured over every bearing of a
/// thousand towns of every size, stretch and seed rather than reasoned
/// about, because the lobes are noise and a bound on noise reasoned from
/// its own gradient is four times what it ever reaches.
#[test]
fn the_outline_never_moves_faster_than_the_bound() {
    const STEP: f64 = 1e-5;
    let mut worst = 0.0f64;
    for k in 0..1000u32 {
        let seed = super::town_seed(7, k as usize);
        let radius = 20.0 + (k % 23) as f64 * 11.0;
        let turn = k as f64 * 0.7;
        let along = if k % 5 == 0 {
            DVec2::ZERO
        } else {
            DVec2::new(turn.cos(), turn.sin())
        };
        let bearing = |a: f64| DVec2::new(a.cos(), a.sin());
        for t in 0..2000 {
            let a = t as f64 / 2000.0 * std::f64::consts::TAU;
            let here = super::edge(bearing(a), radius, along, seed);
            let next = super::edge(bearing(a + STEP), radius, along, seed);
            // Metres of EDGE per metre of ARC: turning the bearing by
            // `STEP` walks the outline's own point by `edge * STEP`.
            worst = worst.max((next - here).abs() / (STEP * here.min(next)));
        }
    }
    println!("the outline moves at most {worst:.3} m of edge a metre of arc");
    assert!(
        worst < super::WOBBLE,
        "an outline moving at {worst:.3} a metre needs a skirt wider than WOBBLE {:.3} allows",
        super::WOBBLE
    );
}

/// A town's levelled ground FOLLOWS its outline, and every lot stands
/// on it.
///
/// The first cut levelled `radius * OUTLINE + APRON` right round every
/// town, which is a DISC, and a town is not one: it is stretched by
/// `STRETCH` along its own shore and squeezed by the same across it, so
/// along the squeezed axis the disc reaches more than twice as far as
/// the town ever does. What that leaves on the ground is a flat apron
/// wider than the town standing on it, which is what the owner read off
/// the climb as big flat discs.
///
/// Both halves matter and only together: levelling less than the town
/// is a building on bare relief with its base at the town's level, which
/// is the buried suburb this file already has a test for.
#[test]
fn a_towns_plateau_follows_its_outline_and_not_a_disc() {
    let planet = planet();
    // Towns of eighty metres or so, because the apron is a block and a
    // street wide now and on a thirty metre hamlet it is the whole of
    // the plateau.
    let towns = plan(&planet, 996.0, 250.0, 3, 7);
    let t = &towns[0];
    let mut here = planet.clone();
    here.sites = vec![site_of(t)].into();
    let (east, north) = frame_at(t.dir);
    let at = |x: f64, z: f64| (t.dir * planet.radius + east * x + north * z).normalize();
    // `keep` is how much of the bare relief is left at a direction, so
    // nought is ground the site levels outright.
    for l in &t.lots {
        let keep = here.surface_blend(at(l.x, l.z)).1;
        assert!(
            keep <= 0.0,
            "a lot {:.0} m out stands on {keep:.3} of bare relief",
            l.x.hypot(l.z)
        );
    }
    let disc = t.radius * OUTLINE + APRON;
    let (mut flat, mut all) = (0.0f64, 0.0f64);
    let (mut widest, mut narrowest) = (0.0f64, f64::MAX);
    const RINGS: usize = 200;
    const BEARINGS: usize = 360;
    for k in 0..BEARINGS {
        let a = k as f64 / BEARINGS as f64 * std::f64::consts::TAU;
        let mut reach = 0.0f64;
        for i in 0..RINGS {
            // Weighted by `r`, because a ring's own area is `r dr dth`.
            let r = (i as f64 + 0.5) / RINGS as f64 * disc;
            all += r;
            if here.surface_blend(at(r * a.cos(), r * a.sin())).1 <= 0.0 {
                flat += r;
                reach = r;
            }
        }
        widest = widest.max(reach);
        narrowest = narrowest.min(reach);
    }
    let share = flat / all;
    println!(
        "the port levels {:.0}% of the disc's own area, {narrowest:.0} m out at its narrowest and {widest:.0} at its widest, against a disc of {disc:.0}",
        share * 100.0
    );
    assert!(
        share < 0.7,
        "the plateau is {:.0}% of the disc: still a disc",
        share * 100.0
    );
    assert!(
        widest > narrowest * 1.5,
        "the plateau runs {widest:.0} m one way and {narrowest:.0} the other: a disc"
    );
}

/// How far a building's own SOLID boxes reach from the middle of its lot,
/// metres east and north: what stands in the road, rather than what hangs
/// over it. An eave, a parapet and a pane are `Model::trim` and a body
/// passes through them, which is this crate's own rule about what a box
/// is for.
fn footprint(m: &crate::model::Model) -> DVec2 {
    let mut half = DVec2::ZERO;
    for s in &m.solids {
        let a = s.axes();
        let (x, y) = (
            (a[0].x * s.half.x).abs() + (a[1].x * s.half.y).abs(),
            (a[0].y * s.half.x).abs() + (a[1].y * s.half.y).abs(),
        );
        half.x = half.x.max(s.centre.x.abs() + x);
        half.y = half.y.max(s.centre.y.abs() + y);
    }
    half
}

/// How far a lot's footprint reaches INTO the paving a town laid, metres:
/// the penetration of two boxes, which is the smaller of their two
/// overlaps, and nought where the building clears every piece.
fn into_street(town: &Town, lot: &Lot, half: DVec2) -> f64 {
    let mut worst: f64 = 0.0;
    for p in &town.pieces {
        let dx = (half.x + p.w * 0.5) - (lot.x - p.x).abs();
        let dz = (half.y + p.d * 0.5) - (lot.z - p.z).abs();
        if dx > 0.0 && dz > 0.0 {
            worst = worst.max(dx.min(dz));
        }
    }
    worst
}

/// A wall is one oriented box that is DRAWN and COLLIDED, so a wall
/// standing on a pavement is a wall a body walks into in the middle of
/// the road. A lot is `LOT` across, the street's own inner kerb is
/// exactly `BLOCK / 2` from its block's middle, and a building on a lot
/// of two by two reaches the kerb and no further, so what a building
/// may cover is its own lot and nothing past it. Measured on a city, a
/// town and the fixture's own villages, because the three tiers lay
/// three different rings.
#[test]
fn a_building_stands_on_its_own_block_and_never_in_the_street() {
    let planet = planet();
    let mut towns = plan(&planet, 996.0, 250.0, 4, 7);
    assert!(!towns.is_empty());
    for (k, radius) in [537.0, 250.0].into_iter().enumerate() {
        towns.push(lay(DVec3::Y, 0.0, radius, DVec2::new(1.0, 0.0), 100 + k, 7));
    }
    for town in &towns {
        let (mut worst, mut where_) = (0.0f64, None);
        let mut over = 0usize;
        for lot in &town.lots {
            let m = crate::model::building(lot.kind, lot.w, lot.w, lot.storeys, town.seed ^ lot.id);
            let into = into_street(town, lot, footprint(&m));
            if into > 0.01 {
                over += 1;
            }
            if into > worst {
                worst = into;
                where_ = Some((lot.kind, lot.x, lot.z));
            }
        }
        println!(
            "town {} of {:.0} m: {} lots, {over} in the street, worst {worst:.2} m at {where_:?}",
            town.index,
            town.radius,
            town.lots.len()
        );
        assert!(
            worst <= 0.01,
            "town {} stands a building {worst:.2} m into its own street",
            town.index
        );
    }
}

/// What a settlement is MADE of, by its TIER, which is the owner's own
/// three: a city with a downtown of towers and streets of two and three
/// storey buildings, a town whose downtown is one and two storey shops,
/// and a village of nothing but one storey houses. By the COUNT of
/// buildings and never by the area a zone covers, which are different
/// numbers because a suburb leaves lots empty and a downtown block
/// carries four buildings where a street block carries twelve.
#[test]
fn a_city_has_towers_a_town_has_shops_and_a_village_has_houses() {
    for (radius, tier) in [
        (537.0, Tier::City),
        (250.0, Tier::Town),
        (150.0, Tier::Village),
    ] {
        assert_eq!(Tier::of(radius), tier);
        let town = lay(DVec3::Y, 0.0, radius, DVec2::new(1.0, 0.0), 0, 7);
        let mut by: [usize; 10] = [0; 10];
        for lot in &town.lots {
            by[(lot.storeys as usize).min(9)] += 1;
        }
        let all = town.lots.len().max(1);
        let over_one: usize = by[2..].iter().sum();
        let over_three: usize = by[4..].iter().sum();
        let big = town.lots.iter().filter(|l| l.w > LOT).count();
        println!(
            "a {radius:.0} m {}: {} lots of which {big} are two by two, {} pieces, storeys {by:?}, {:.1}% over one storey and {:.1}% over three",
            tier.name(),
            town.lots.len(),
            town.pieces.len(),
            100.0 * over_one as f64 / all as f64,
            100.0 * over_three as f64 / all as f64,
        );
        match tier {
            Tier::City => {
                // Towers downtown and a few storeys on the high street:
                // a quarter to a half of the buildings stand over one
                // storey and the towers are a minority of those.
                let share = over_one as f64 / all as f64;
                assert!(
                    (0.2..=0.5).contains(&share),
                    "a city is {:.1}% over one storey",
                    share * 100.0
                );
                assert!(
                    over_three > 0 && over_three < over_one,
                    "no towers, or nothing but"
                );
                assert!(big > 0, "a city with no two by two building in it");
                assert!(
                    town.pieces.iter().any(|p| p.square()),
                    "a city with no square"
                );
            }
            Tier::Town => {
                assert!(over_one > 0, "a town with no two storey shop in it");
                assert_eq!(over_three, 0, "a town with a tower in it");
                assert!(
                    town.lots.iter().all(|l| l.storeys <= 2),
                    "over two storeys in a town"
                );
                assert!(
                    big > 0,
                    "a town with no two by two building round its square"
                );
                assert!(
                    town.pieces.iter().any(|p| p.square()),
                    "a town with no square"
                );
            }
            Tier::Village => {
                assert_eq!(over_one, 0, "a village with something over one storey");
                assert_eq!(big, 0, "a village with a two by two building in it");
                assert!(
                    !town.pieces.iter().any(|p| p.square()),
                    "a village with a square"
                );
                assert!(town
                    .lots
                    .iter()
                    .all(|l| matches!(l.kind, Kind::House | Kind::Bungalow | Kind::Hangar)));
            }
        }
    }
}
