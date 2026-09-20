use super::*;
use crate::town;

/// A ring of directions to ask the clock about, so nothing here passes on
/// one lucky longitude.
fn places() -> Vec<DVec3> {
    (0..12)
        .map(|i| {
            let a = TAU * i as f64 / 12.0;
            let lat = (i as f64 / 11.0 - 0.5) * 1.2;
            DVec3::new(a.cos() * lat.cos(), lat.sin(), a.sin() * lat.cos()).normalize()
        })
        .collect()
}

#[test]
fn a_full_turn_brings_the_sun_back_to_where_it_started() {
    let noon = DVec3::new(0.4, 0.3, -0.87).normalize();
    let back = sun_at(noon, DAY, DAY);
    assert!(
        back.distance(noon) < 1e-12,
        "a day round: {back} against {noon}"
    );
    // And half a day is the far side of the axis, which is midnight.
    let mid = sun_at(noon, DAY * 0.5, DAY);
    assert!((mid.x + noon.x).abs() < 1e-12 && (mid.z + noon.z).abs() < 1e-12);
    assert!((mid.y - noon.y).abs() < 1e-12, "the declination is held");
}

#[test]
fn a_turn_holds_the_suns_latitude_and_so_a_body_keeps_its_seasons() {
    let noon = DVec3::new(0.2, 0.55, 0.81).normalize();
    for step in 0..24 {
        let sun = sun_at(noon, DAY * step as f64 / 24.0, DAY);
        assert!(
            (sun.y - noon.y).abs() < 1e-12,
            "step {step}: {} against {}",
            sun.y,
            noon.y
        );
        assert!((sun.length() - 1.0).abs() < 1e-12);
    }
}

#[test]
fn the_sun_crosses_the_sky_from_east_to_west() {
    // East at a direction is `AXIS cross up`, which is what every frame in
    // this crate is built on (`town::frame_at`), so the one honest test of
    // which way the sun goes is against that east: it stands east of the
    // meridian before noon and west of it after, and the crossing is the
    // moment it is highest.
    for dir in places() {
        let (east, _) = town::frame_at(dir);
        let noon = DVec3::new(0.1, 0.25, 0.96).normalize();
        let midday = at_oclock(noon, dir, 12.0, DAY);
        let before = sun_at(noon, midday - DAY / 24.0, DAY).dot(east);
        let after = sun_at(noon, midday + DAY / 24.0, DAY).dot(east);
        assert!(
            before > 0.0 && after < 0.0,
            "at {dir}: an hour before noon the sun is {before:.4} east and an hour after {after:.4}"
        );
    }
}

#[test]
fn the_clock_runs_forwards_and_reads_twelve_at_the_suns_own_noon() {
    let noon = DVec3::new(-0.3, 0.2, 0.93).normalize();
    for dir in places() {
        let midday = at_oclock(noon, dir, 12.0, DAY);
        let sun = sun_at(noon, midday, DAY);
        assert!(
            (oclock(sun, dir) - 12.0).abs() < 1e-6,
            "at {dir} the clock reads {} at the sun's own noon",
            oclock(sun, dir)
        );
        // And it is a MAXIMUM of the elevation, which is what noon means.
        let peak = elevation(sun, dir);
        for step in [-0.2, -0.05, 0.05, 0.2] {
            let other = sun_at(noon, midday + DAY * step, DAY);
            assert!(
                elevation(other, dir) < peak + 1e-9,
                "at {dir} the sun is higher {step} of a day off its own noon"
            );
        }
        // An hour on, the clock says an hour on.
        let later = sun_at(noon, midday + DAY / 24.0, DAY);
        assert!((oclock(later, dir) - 13.0).abs() < 1e-6);
    }
}

#[test]
fn every_place_on_a_body_gets_a_night_and_a_day() {
    // A sun with no declination stands over the equator, so every
    // latitude has both. What this is really holding is that the turn
    // is about the POLE: turned about any other axis the poles would be
    // in perpetual day or perpetual night at once.
    let noon = DVec3::new(1.0, 0.0, 0.0);
    for dir in places() {
        let (mut up, mut down) = (false, false);
        for step in 0..48 {
            let sun = sun_at(noon, DAY * step as f64 / 48.0, DAY);
            let e = elevation(sun, dir);
            up |= e > 0.05;
            down |= e < -0.05;
        }
        assert!(up && down, "at {dir}: day {up}, night {down}");
    }
}

#[test]
fn an_hour_asked_for_is_the_hour_that_arrives() {
    let noon = DVec3::new(0.51, -0.4, 0.76).normalize();
    let dir = DVec3::new(0.3, 0.2, -0.93).normalize();
    for want in [0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 18.0, 21.0, 23.5] {
        let t = at_oclock(noon, dir, want, DAY);
        assert!((0.0..DAY).contains(&t), "{want}: {t} s is off the day");
        let read = oclock(sun_at(noon, t, DAY), dir);
        let off = (read - want).abs().min(24.0 - (read - want).abs());
        assert!(off < 1e-6, "asked for {want} and got {read}");
    }
}

#[test]
fn a_clock_with_nothing_to_measure_still_answers() {
    // The sun straight up the pole: every hour is the same hour there and
    // `highest` has no maximum to find, so it answers nought rather than
    // a NaN out of `atan2(0, 0)`. A NaN in the sun is every frame after
    // it wrong.
    assert_eq!(highest(AXIS, AXIS), 0.0);
    assert!(oclock(AXIS, AXIS).is_finite());
    assert!(sun_at(DVec3::ZERO, 10.0, DAY).is_finite());
    assert_eq!(hour(f64::NAN, DAY), 0.0);
    assert_eq!(hour(10.0, 0.0), 0.0);
    assert!(elevation(DVec3::ZERO, DVec3::ZERO).is_finite());
}

#[test]
fn the_lamps_come_on_as_the_sun_goes_down_and_not_before() {
    let dir = DVec3::new(0.3, 0.2, -0.93).normalize();
    let noon = DVec3::new(0.1, 0.25, 0.96).normalize();
    let at = |h: f64| sun_at(noon, at_oclock(noon, dir, h, DAY), DAY);
    // Noon: full day, no lamps. Midnight: full night, lamps hard on.
    assert!((daylight(at(12.0), dir) - 1.0).abs() < 1e-9);
    assert!(lamplight(at(12.0), dir) < 1e-9);
    assert!(daylight(at(0.0), dir) < 1e-9);
    assert!((lamplight(at(0.0), dir) - 1.0).abs() < 1e-9);
    // And it is MONOTONE through the evening: a lamp that flickered on
    // and off across dusk would be a lamp nobody believes.
    let mut last = 0.0;
    for step in 0..40 {
        let h = 12.0 + 12.0 * step as f64 / 39.0;
        let now = lamplight(at(h), dir);
        assert!(now >= last - 1e-12, "at {h:.1} o'clock: {now} after {last}");
        last = now;
    }
    // The band is a BAND: somewhere in the evening it is half lit.
    assert!(
        (0..240).any(|s| {
            let l = lamplight(at(12.0 + 12.0 * s as f64 / 239.0), dir);
            (0.3..0.7).contains(&l)
        }),
        "dusk is a step rather than a band"
    );
}
