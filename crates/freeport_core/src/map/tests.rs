use super::*;
use crate::town;

/// The harness body's radius, metres.
const R: f64 = 1_000_000.0;

/// A view goes to a pixel and back to the same place, to well under a
/// millimetre, across a whole window at the scale a region is drawn.
#[test]
fn a_view_goes_to_a_pixel_and_back() {
    let view = View::new(DVec3::new(0.3, 0.8, -0.2), R, 48.0);
    let mut worst: f64 = 0.0;
    for i in -8..=8 {
        for j in -5..=5 {
            let px = DVec2::new(i as f64 * 80.0, j as f64 * 70.0);
            let back = view.to_px(view.to_dir(px)).expect("in the view");
            worst = worst.max((back - px).length() * view.scale);
        }
    }
    assert!(worst < 1e-3, "{worst} m out");
    // North is up and east is right.
    let (east, north) = frame_at(view.centre);
    let step = |d: DVec3| {
        view.to_px((view.centre + d * 1e-4).normalize())
            .expect("near")
    };
    assert!(step(east).x > 0.0 && step(east).y.abs() < 1e-9);
    assert!(step(north).y > 0.0 && step(north).x.abs() < 1e-9);
}

/// A small planet with a coast, towns and roads on it: the road tests'
/// own world, so what is drawn is what `connect` routed.
fn world() -> (crate::field::Planet, f64, Vec<Town>, Vec<crate::road::Road>) {
    let mut planet = crate::field::Planet {
        radius: 40_000.0,
        relief: 400.0,
        lumps: 8.0,
        octaves: 9,
        overhang: 0.0,
        ledge: 0.0,
        seed: 5,
        sites: vec![].into(),
    };
    let sea = planet.radius - 40.0;
    let towns = town::plan(&planet, sea, 250.0, 12, 5);
    planet.sites = towns.iter().map(town::site_of).collect();
    let roads = crate::road::connect(&planet, sea, &towns, 0.06);
    (planet, sea, towns, roads)
}

/// The scene of that world: the bare planet, the roads on their own
/// centrelines with tarmac all along, and the towns.
fn lines_of(roads: &[crate::road::Road], radius: f64) -> (Vec<Vec<DVec3>>, Vec<Vec<bool>>) {
    let lines: Vec<Vec<DVec3>> = roads
        .iter()
        .map(|r| crate::road::centreline(r, radius))
        .collect();
    let open = lines.iter().map(|l| vec![true; l.len()]).collect();
    (lines, open)
}

/// A map of a real routed planet, wide enough to hold both: the sea is
/// drawn blue where the ground is under it and land where it is not.
#[test]
fn a_map_draws_the_sea_where_the_ground_is_under_it() {
    let (planet, sea, towns, roads) = world();
    let bare = planet.bare();
    let (lines, open) = lines_of(&roads, planet.radius);
    let refs: Vec<Line> = lines
        .iter()
        .zip(&open)
        .map(|(points, open)| Line { points, open })
        .collect();
    let scene = Scene {
        planet: &bare,
        sea,
        roads: &refs,
        towns: &towns,
    };
    let view = View::new(towns[0].dir, planet.radius, 90.0);
    let (w, h) = (320, 200);
    let t0 = std::time::Instant::now();
    let pic = draw(&scene, &view, w, h, 4);
    let took = t0.elapsed();
    let heights = paint::heights(&scene, &view, w, h, 4);
    let (mut wet, mut blue, mut dry, mut green) = (0, 0, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let [r, _, b, a] = pic.pixel(x, y);
            assert_eq!(a, 255, "the picture is opaque");
            if heights[y * w + x] < -5.0 {
                wet += 1;
                blue += usize::from(b > r);
            } else if heights[y * w + x] > 5.0 {
                dry += 1;
                green += usize::from(b <= r + 8);
            }
        }
    }
    println!(
        "{w} by {h} in {:.1} ms: {wet} wet pixels, {blue} of them blue; {dry} dry, {green} of them not",
        took.as_secs_f64() * 1000.0
    );
    assert!(wet > 500 && dry > 500, "the view holds both sea and land");
    assert!(blue * 10 >= wet * 9, "the sea is drawn as sea");
    assert!(green * 10 >= dry * 9, "the land is drawn as land");
}

/// Close in, a town is drawn as its streets and buildings, and a road
/// out in the country as a line in the road's own colour.
#[test]
fn a_map_draws_a_towns_plan_and_its_roads_where_they_are() {
    let (planet, sea, towns, roads) = world();
    let bare = planet.bare();
    let (lines, open) = lines_of(&roads, planet.radius);
    let refs: Vec<Line> = lines
        .iter()
        .zip(&open)
        .map(|(points, open)| Line { points, open })
        .collect();
    let scene = Scene {
        planet: &bare,
        sea,
        roads: &refs,
        towns: &towns,
    };
    let first = &towns[0];
    let view = View::new(first.dir, planet.radius, 4.0);
    let (w, h) = (320, 200);
    let pic = draw(&scene, &view, w, h, 4);
    // The same view with no towns in it: what differs is what the town
    // drew, which a colour match cannot tell from a road or a sunlit
    // slope of the same grey.
    let empty = Scene {
        towns: &[],
        ..scene
    };
    let bare_pic = draw(&empty, &view, w, h, 4);
    let changed = (0..w * h)
        .filter(|i| (0..3).any(|c| pic.rgba[i * 4 + c].abs_diff(bare_pic.rgba[i * 4 + c]) > 10))
        .count();
    // What the town's own plan says it covers, pixels.
    let px = |m2: f64| m2 / (view.scale * view.scale);
    let built: f64 = first.lots.iter().map(|l| px((l.w * BUILT).powi(2))).sum();
    let paved: f64 = first.pieces.iter().map(|p| px(p.w * p.d)).sum();
    println!(
        "town 0 changed {changed} pixels against {:.0} its plan covers ({built:.0} built, {paved:.0} paved)",
        built + paved
    );
    assert!(changed as f64 > 0.8 * (built + paved), "the town is drawn");
    assert!(
        (changed as f64) < 1.3 * (built + paved),
        "and only the town"
    );
    let near =
        |c: [u8; 4], d: paint::Colour| (0..3).all(|i| (c[i] as i32 - d[i] as i32).abs() < 24);
    // A road's own points out in the country, a few kilometres from any
    // town, are drawn in the road's colour: a view on the middle of the
    // longest road, which is country wherever the towns came out.
    let longest = lines.iter().max_by_key(|l| l.len()).expect("a road");
    let wide = View::new(longest[longest.len() / 2], planet.radius, 30.0);
    let pic = draw(&scene, &wide, w, h, 4);
    let (mut asked, mut hit) = (0, 0);
    for line in &lines {
        for p in line.iter().step_by(7) {
            if towns
                .iter()
                .any(|t| t.dir.angle_between(*p) * planet.radius < 2_500.0)
            {
                continue;
            }
            let Some(px) = wide.to_px(*p) else { continue };
            let q = paint::at(px, w, h);
            if q.x < 2.0 || q.y < 2.0 || q.x > w as f64 - 2.0 || q.y > h as f64 - 2.0 {
                continue;
            }
            asked += 1;
            hit += usize::from(near(pic.pixel(q.x as usize, q.y as usize), paint::ROAD));
        }
    }
    println!("{hit} of {asked} country road points drawn as road");
    assert!(asked > 10, "the view holds some country road");
    assert!(hit * 10 >= asked * 9, "a road is drawn where it runs");
}

/// Far out, a town whose lots are under a pixel is drawn as its
/// BUILT-UP AREA: every built block's cell in the town's own colour, so
/// what the picture shows is where the town is and how big, and not a
/// grey smudge of lots each a fifth of a pixel. Close in there is none of
/// it, and the courtyards between the lots are the ground.
#[test]
fn a_town_too_fine_to_draw_is_drawn_as_its_built_up_area() {
    assert_eq!(built_up(48.0), 1.0);
    assert_eq!(built_up(4.0), 0.0);
    let (planet, sea, towns, _) = world();
    let bare = planet.bare();
    let scene = Scene {
        planet: &bare,
        sea,
        roads: &[],
        towns: &towns,
    };
    let first = &towns[0];
    let view = View::new(first.dir, planet.radius, 12.0);
    let (w, h) = (160, 160);
    let pic = draw(&scene, &view, w, h, 4);
    let empty = Scene {
        towns: &[],
        ..scene
    };
    let bare_pic = draw(&empty, &view, w, h, 4);
    let changed = (0..w * h)
        .filter(|i| (0..3).any(|c| pic.rgba[i * 4 + c].abs_diff(bare_pic.rgba[i * 4 + c]) > 10))
        .count();
    let built = blocks_of(first);
    let side = PITCH / view.scale;
    let cells = built.len() as f64 * side * side;
    // Where a cell has no built neighbour, the street round it and the
    // pixels the edge only partly covers make a ring a pixel or so wide:
    // what "nothing past it" can honestly allow, and it grows with how
    // ragged the town is rather than with its area.
    let open = built
        .iter()
        .map(|&(x, z)| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter(|(dx, dz)| !built.contains(&(x + dx, z + dz)))
                .count()
        })
        .sum::<usize>() as f64;
    let ring = open * side * 1.5;
    println!(
        "town 0 at {} m a pixel changed {changed} pixels against {cells:.0} of built-up area \
         and a ring of {ring:.0} round its {open} open edges",
        view.scale
    );
    assert!(changed as f64 > 0.85 * cells, "the town's area is drawn");
    assert!((changed as f64) < cells + ring, "and nothing past it");
    // Its built middle reads as the town and not as the ground: the
    // built block nearest its middle, because the middle itself is its
    // market square wherever the town is big enough to have one.
    let &(bx, bz) = built
        .iter()
        .min_by_key(|(x, z)| x * x + z * z)
        .expect("a built block");
    let at = town::lot_frame(planet.radius, first, bx as f64 * PITCH, bz as f64 * PITCH).dir;
    let mid = paint::at(view.to_px(at).expect("in view"), w, h);
    let c = pic.pixel(mid.x as usize, mid.y as usize);
    let want = paint::building(Tier::of(first.radius));
    assert!(
        (0..3).all(|i| (c[i] as i32 - want[i] as i32).abs() < 60),
        "{c:?} against {want:?}"
    );
}

/// A road is drawn at its own width once the map is close enough to
/// show it, and never thinner than two pixels.
#[test]
fn a_road_is_as_wide_as_the_ground_or_two_pixels() {
    let wide = |scale: f64| (ROAD_M / scale).max(ROAD_PX);
    assert_eq!(wide(48.0), ROAD_PX);
    assert!((wide(1.0) - ROAD_M).abs() < 1e-9);
    assert!((ROAD_M - 6.9).abs() < 1e-9, "{ROAD_M} m of road");
}

/// A contour is drawn at an interval that leaves room between two lines:
/// a hundred metres at a region's scale and five at a street's, and
/// every line on the picture is on the HIGHER side of the height it
/// marks, so a slope's lines are one pixel wide.
#[test]
fn a_contour_interval_follows_the_scale_and_its_lines_are_one_pixel() {
    assert_eq!(contour(48.0), 100.0);
    assert_eq!(contour(8.0), 20.0);
    assert_eq!(contour(2.0), 5.0);
    assert_eq!(contour(2_000.0), 500.0);
    // A plane rising a metre a pixel to the east, at a metre a pixel: a
    // five metre interval puts a line every fifth column and no wider.
    let (w, h) = (40, 4);
    let heights: Vec<f64> = (0..w * h).map(|i| 3.0 + (i % w) as f64).collect();
    let pic = paint::ground(&heights, w, h, 1.0);
    let row: Vec<u8> = (0..w).map(|x| pic.pixel(x, 1)[1]).collect();
    let darker: Vec<usize> = (1..w - 1)
        .filter(|x| row[*x] < row[x - 1].min(row[x + 1]))
        .collect();
    println!("contoured columns {darker:?}");
    assert_eq!(darker, vec![2, 7, 12, 17, 22, 27, 32, 37]);
}
