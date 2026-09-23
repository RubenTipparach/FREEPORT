//! What a car does when it is HIT: `Driver::knock` and the slide and
//! spin it leaves, on the same ball the driving tests use.

use super::tests::{bounds, car, drive, world, R};
use super::*;

#[test]
fn a_knocked_car_slides_sideways_turns_and_stops_where_the_tyres_leave_it() {
    let (b, w) = (bounds(), world(&[]));
    let mut d = car(&w, &b);
    // Shoved 10 m/s to its right and turned, with nobody on the pedals.
    let right = d.right();
    d.knock(right * 10.0 + d.fwd * 4.0, 1.0);
    assert!(
        (d.speed - 4.0).abs() < 1e-9,
        "the along part is speed: {}",
        d.speed
    );
    assert!(
        (d.shove.length() - 10.0).abs() < 1e-9,
        "the across part is a shove"
    );
    let from = d.dir;
    let heading = d.fwd;
    drive(&mut d, &w, &b, Drive::default(), 4.0);
    let across = (d.dir - from).dot(right) * R;
    // A slide at 10 m/s worn at SKID stops in 10^2 / (2 * 6) = 8.3 m.
    println!(
        "slid {across:.2} m across and turned {:.1} degrees",
        heading.angle_between(d.fwd).to_degrees()
    );
    assert!((7.0..10.0).contains(&across), "slid {across:.2} m");
    assert!(
        d.shove.length() < 1e-9 && d.spin == 0.0,
        "still sliding: {:?} {}",
        d.shove,
        d.spin
    );
    assert!(d.speed.abs() < 1e-6, "still rolling at {}", d.speed);
    let turned = heading.angle_between(d.fwd).to_degrees();
    // A spin of one radian a second worn at SPIN_DRAG turns 1 / (2 * 3) rad.
    assert!((5.0..15.0).contains(&turned), "turned {turned:.1} degrees");
}

#[test]
fn a_rammed_car_takes_the_speed_and_the_rammer_loses_it_by_the_same_amount() {
    let (b, w) = (bounds(), world(&[]));
    let mut hit = car(&w, &b);
    let mut ram = car(&w, &b);
    ram.speed = 16.0;
    let a = crate::ram::Body {
        at: ram.dir * ram.foot,
        fwd: ram.fwd,
        vel: ram.velocity(),
    };
    // The struck car a hand ahead of the rammer's nose, pointing the same way.
    hit.dir = (ram.dir + ram.fwd * ((crate::figure::CAR_LONG + 0.1) / R)).normalize();
    hit.fwd = (ram.fwd - hit.dir * ram.fwd.dot(hit.dir)).normalize();
    let target = crate::ram::Body {
        at: hit.dir * hit.foot,
        fwd: hit.fwd,
        vel: hit.velocity(),
    };
    let h = crate::ram::impact(&a, &target).expect("touching and closing");
    hit.knock(h.normal * h.dv, h.spin);
    ram.knock(-h.normal * h.dv, 0.0);
    println!(
        "the rammer goes on at {:.2} m/s and the struck car at {:.2}",
        ram.speed, hit.speed
    );
    // To a part in a hundred thousand: the two stand 4 m apart on a 2 km
    // ball, so the normal is two milliradians off the struck car's own
    // tangent plane and that much of the knock goes into its shove.
    assert!(
        (ram.speed + hit.speed - 16.0).abs() < 1e-4,
        "momentum was not kept: {} and {}",
        ram.speed,
        hit.speed
    );
    assert!(
        hit.speed > ram.speed,
        "the struck car is the faster of the two after a rear end"
    );
    assert!(hit.shove.length() < 1e-9, "a square hit shoved it sideways");
}
