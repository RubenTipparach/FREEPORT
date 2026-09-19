use super::*;
use crate::field::{Block, Built, Sphere, CONCRETE};

/// A ball big enough that the ground under a few seconds of driving is
/// flat: this is the walker suite's own radius and for the same reason,
/// which is that on a small ball a block three metres out stands higher
/// than the ground curving away under it.
const R: f64 = 2000.0;

fn bounds() -> Bounds {
    Bounds {
        radius: R,
        floor: R - 50.0,
        top: R + 50.0,
        sea: 0.0,
    }
}

/// The ball, with whatever boxes are handed in standing on it.
fn world(blocks: &[Block]) -> Built<'_> {
    Built {
        ground: Box::leak(Box::new(Sphere { radius: R })),
        blocks: blocks.iter().collect(),
    }
}

/// A car set down at the north pole pointing along x.
fn car(field: &dyn Density, b: &Bounds) -> Driver {
    Driver::board(field, b, DVec3::Y, DVec3::X)
}

/// Drive for `secs` with the pedals held, a sixtieth at a time, and say
/// how far along the ground it got.
fn drive(d: &mut Driver, field: &dyn Density, b: &Bounds, input: Drive, secs: f64) -> f64 {
    let from = d.dir;
    for _ in 0..(secs * 60.0) as usize {
        d.update(field, b, &input, 1.0 / 60.0);
    }
    from.angle_between(d.dir) * R
}

#[test]
fn a_car_pulls_away_and_tops_out() {
    let (b, w) = (bounds(), world(&[]));
    let mut d = car(&w, &b);
    let throttle = Drive {
        throttle: 1.0,
        ..Default::default()
    };
    // It ACCELERATES rather than arriving at its speed: a second of
    // throttle is `ACCEL` and no more, which is what gives a car weight.
    drive(&mut d, &w, &b, throttle, 1.0);
    println!("a second of throttle reaches {:.2} m/s", d.speed);
    assert!(
        (ACCEL * 0.9..=ACCEL * 1.1).contains(&d.speed),
        "a second of throttle reached {:.2} m/s, not about {ACCEL}",
        d.speed
    );
    let gone = drive(&mut d, &w, &b, throttle, 8.0);
    println!("eight more seconds goes {gone:.1} m at {:.2} m/s", d.speed);
    assert!(
        (TOP - 0.01..=TOP).contains(&d.speed),
        "it tops out at {:.2} m/s, not {TOP}",
        d.speed
    );
    // And it STOPS, on the brake, in the distance a brake implies.
    let stopping = drive(
        &mut d,
        &w,
        &b,
        Drive {
            brake: true,
            ..Default::default()
        },
        3.0,
    );
    println!("the brake takes {TOP} m/s off in {stopping:.1} m");
    assert_eq!(d.speed, 0.0, "the brake left it rolling");
    assert!(
        (8.0..14.0).contains(&stopping),
        "it stopped in {stopping:.1} m from {TOP} m/s"
    );
    // Coasting is slower than braking and still comes to rest.
    let mut d = car(&w, &b);
    drive(&mut d, &w, &b, throttle, 6.0);
    let was = d.speed;
    drive(&mut d, &w, &b, Drive::default(), 3.0);
    assert!(d.speed < was && d.speed > 0.0, "coasting is not coasting");
}

/// The ONE thing that makes this a car and not a fast walker: it steers
/// with its wheels, so the yaw rate is the speed over the turning radius
/// and a car standing still cannot turn at all.
#[test]
fn a_car_standing_still_cannot_turn_however_hard_the_wheel_is_held() {
    let (b, w) = (bounds(), world(&[]));
    let mut d = car(&w, &b);
    let was = d.fwd;
    let held = Drive {
        steer: 1.0,
        ..Default::default()
    };
    drive(&mut d, &w, &b, held, 3.0);
    let turned = was.angle_between(d.fwd).to_degrees();
    println!("three seconds of full lock standing still turns {turned:.4} degrees");
    assert!(turned < 1e-6, "a parked car turned {turned:.4} degrees");
    // Rolling, the same wheel turns it, and FASTER the faster it goes.
    let rate = |speed: f64| {
        let mut d = car(&w, &b);
        d.speed = speed;
        d.yaw_rate(1.0).abs()
    };
    let (slow, fast) = (rate(2.0), rate(8.0));
    println!("full lock yaws {slow:.3} rad/s at 2 m/s and {fast:.3} at 8");
    assert!(fast > slow, "the wheel does the same at every speed");
}

/// How tight a circle it can hold: the wheelbase over the tangent of the
/// lock, which is a small car's ten metre turning circle. Measured by
/// DRIVING one rather than by reading the constant back.
#[test]
fn a_car_turns_the_circle_its_wheelbase_says() {
    let (b, w) = (bounds(), world(&[]));
    let mut d = car(&w, &b);
    // At a crawl, where the lock is not yet tapered off.
    d.speed = 1.5;
    let held = Drive {
        steer: 1.0,
        ..Default::default()
    };
    let from = d.dir;
    let start = d.fwd;
    // A quarter turn of the wheel's own circle.
    let mut turned = 0.0;
    let mut gone = 0.0;
    while turned < std::f64::consts::FRAC_PI_2 && gone < 60.0 {
        let was = d.fwd;
        d.speed = 1.5;
        d.update(&w, &b, &held, 1.0 / 60.0);
        turned += was.angle_between(d.fwd);
        gone += 1.5 / 60.0;
    }
    // Arc length over the angle swept is the radius.
    let radius = gone / turned;
    let want = WHEELBASE / (LOCK / (1.0 + 1.5 / TAPER)).tan();
    println!("it holds a {radius:.2} m circle, against the {want:.2} m the wheelbase says");
    assert!(
        (radius - want).abs() < 0.25,
        "a {radius:.2} m circle against {want:.2}"
    );
    assert!(from.angle_between(d.dir) * R > 1.0, "it turned on the spot");
    assert!(start.angle_between(d.fwd) > 1.0, "it did not turn at all");
}

/// A wall stops it and a KERB does not, which is the difference between
/// a road and a pavement to something with wheels.
#[test]
fn a_wall_stops_a_car_and_a_kerb_is_driven_up() {
    let b = bounds();
    let frame = crate::town::Frame {
        dir: DVec3::Y,
        east: DVec3::X,
        north: DVec3::Z,
        base: R,
    };
    let block = |along: f64, half: DVec3| {
        crate::model::Solid {
            centre: DVec3::new(along, 0.0, half.z),
            half,
            yaw: 0.0,
            material: CONCRETE,
        }
        .block(&frame)
    };
    // A wall eight metres ahead, tall enough to be a wall.
    let wall = [block(8.0, DVec3::new(0.4, 6.0, 1.6))];
    let w = world(&wall);
    let mut d = car(&w, &b);
    let gone = drive(
        &mut d,
        &w,
        &b,
        Drive {
            throttle: 1.0,
            ..Default::default()
        },
        6.0,
    );
    println!(
        "a car driven at a wall 8 m off gets {gone:.2} m and ends at {:.2} m/s",
        d.speed
    );
    assert!(
        (5.0..8.0).contains(&gone),
        "it went {gone:.2} m at a wall 8 m away, less its own 2.05 m of bonnet"
    );
    assert!(
        d.speed.abs() < 1.0,
        "it drove through at {:.2} m/s",
        d.speed
    );

    // A kerb the same distance off is driven over and the car keeps going.
    let kerb = [block(8.0, DVec3::new(2.0, 6.0, 0.085))];
    let w = world(&kerb);
    let mut d = car(&w, &b);
    let gone = drive(
        &mut d,
        &w,
        &b,
        Drive {
            throttle: 1.0,
            ..Default::default()
        },
        6.0,
    );
    println!(
        "the same car over a 17 cm kerb goes {gone:.1} m at {:.2} m/s",
        d.speed
    );
    assert!(gone > 20.0, "a 17 cm kerb stopped a car at {gone:.2} m");
    assert!(d.speed > 5.0, "a kerb left it at {:.2} m/s", d.speed);
}

/// Reverse is slower than forward, and backing up is backing up.
#[test]
fn a_car_reverses_slowly_and_the_way_it_came() {
    let (b, w) = (bounds(), world(&[]));
    let mut d = car(&w, &b);
    let back = Drive {
        throttle: -1.0,
        ..Default::default()
    };
    drive(&mut d, &w, &b, back, 6.0);
    println!("six seconds of reverse holds {:.2} m/s", d.speed);
    assert!(
        (-REVERSE - 0.01..=-REVERSE + 0.01).contains(&d.speed),
        "reverse tops out at {:.2} m/s, not {}",
        d.speed,
        -REVERSE
    );
    assert!(REVERSE < TOP, "reverse is as fast as forward");
    // It went BACKWARDS: the place it reached is behind where it started.
    let mut back_car = car(&w, &b);
    let from = back_car.dir;
    let facing = back_car.fwd;
    let gone = drive(&mut back_car, &w, &b, back, 2.0);
    let moved = (back_car.dir - from) * R;
    assert!(moved.dot(facing) < -0.5, "reverse went forwards");
    // And it did not TURN. A heading is a tangent vector, so carrying it
    // over a curved surface rotates it by exactly the arc it travelled
    // (10 m on a 2 km ball is 0.005 rad) and that is the ground turning
    // under the car rather than the car turning on it. Anything past
    // that arc would be a steering input nobody asked for.
    let swung = facing.angle_between(back_car.fwd);
    println!(
        "reversing {gone:.1} m swings the heading {swung:.5} rad, the arc being {:.5}",
        gone / R
    );
    assert!(
        swung < gone / R * 1.05 + 1e-9,
        "reversing {gone:.1} m turned the car {swung:.5} rad, past its own arc"
    );
}

/// The driver sits IN the car and gets out BESIDE it, which is what
/// keeps a body from being left standing inside the thing it just left.
#[test]
fn the_seat_is_in_the_car_and_the_door_is_beside_it() {
    let (b, w) = (bounds(), world(&[]));
    let d = car(&w, &b);
    let eye = d.eye();
    assert!(
        (eye.length() - d.foot - SEAT).abs() < 1e-9,
        "the seat is not {SEAT} m over the road"
    );
    assert!(
        SEAT < crate::figure::CAR_WIDE + 0.2,
        "the driver's head is through the roof"
    );
    let out = d.kerbside(R);
    let across = (out - d.dir).dot(d.right()) * R;
    println!("a body gets out {across:.2} m to the RIGHT of the car's middle");
    assert!(
        across > crate::figure::CAR_WIDE * 0.5,
        "getting out leaves the body inside the car, or in the oncoming lane"
    );
    assert!(across < 4.0, "getting out throws the body {across:.1} m");
    // And onto the PAVEMENT: traffic keeps right, so a car rides half a
    // lane right of the centreline and this lands in the 1.5 m of
    // pavement past the lane's own edge.
    let from_middle = crate::town::LANE * 0.5 + across;
    println!("which is {from_middle:.2} m from the street's own centreline");
    assert!(
        (crate::town::LANE..crate::town::STREET * 0.5).contains(&from_middle),
        "a body gets out {from_middle:.2} m off the centreline, not on the pavement"
    );
    assert!(
        (out.length() - 1.0).abs() < 1e-12,
        "a direction that is not a direction"
    );
}

/// A car STEERS TOWARD somewhere, which is what a drive between two
/// towns is made of. The wheel goes hard over for a place behind, eases
/// as the nose comes round, and reads nought dead ahead.
#[test]
fn a_car_steers_toward_where_it_is_going_and_straightens_when_it_is_aimed() {
    let field = world(&[]);
    let bounds = bounds();
    let dir = DVec3::Y;
    let mut car = car(&field, &bounds);
    let ahead = (dir + car.fwd * 0.01).normalize();
    assert!(
        car.toward(ahead).abs() < 0.05,
        "dead ahead asks for {:.3} of lock",
        car.toward(ahead)
    );
    let left = (dir - car.right() * 0.01).normalize();
    let right = (dir + car.right() * 0.01).normalize();
    assert!(
        car.toward(left) > 0.9,
        "hard left is {:.2}",
        car.toward(left)
    );
    assert!(
        car.toward(right) < -0.9,
        "hard right is {:.2}",
        car.toward(right)
    );
    // And driving with the wheel on that bearing CLOSES on the place.
    // Two hundred metres off, which is far enough that closing means
    // something: a car turns inside 4.8 m, so a goal eight metres away
    // is one it orbits rather than arrives at.
    let goal = (dir + car.right() * 0.1).normalize();
    let start = car.dir.angle_between(goal) * R;
    for _ in 0..1800 {
        let steer = car.toward(goal);
        car.update(
            &field,
            &bounds,
            &Drive {
                throttle: 1.0,
                steer,
                brake: false,
            },
            1.0 / 60.0,
        );
    }
    let end = car.dir.angle_between(goal) * R;
    println!("closed from {start:.1} m to {end:.1} m in thirty seconds");
    assert!(end < 20.0, "closed {start:.1} m to {end:.1} m only");
}

/// A car that has met a wall can BACK OFF IT AGAIN.
///
/// The first scripted drive to the next town went twelve metres, met a
/// corner of the port and then never moved again: 4 km/h, then 3, then
/// 2, then 1, and nought metres of ground in the next thirteen minutes.
/// Backing off did not free it either, so the picture at the end was
/// byte for byte the picture at the start.
#[test]
fn a_car_that_has_met_a_wall_can_back_off_it_again() {
    let b = bounds();
    let frame = crate::town::Frame {
        dir: DVec3::Y,
        east: DVec3::X,
        north: DVec3::Z,
        base: R,
    };
    let slab = |centre: DVec3, half: DVec3| {
        crate::model::Solid {
            centre,
            half,
            yaw: 0.0,
            material: CONCRETE,
        }
        .block(&frame)
    };
    // A CORNER, which is what the car in the game actually met: two
    // walls at a right angle, so the outline is pushed out along both
    // and a step that clears one is refused by the other.
    let wall = [
        slab(DVec3::new(8.0, 0.0, 1.6), DVec3::new(0.4, 6.0, 1.6)),
        slab(DVec3::new(4.0, 4.0, 1.6), DVec3::new(6.0, 0.4, 1.6)),
    ];
    let w = world(&wall);
    let mut d = car(&w, &b);
    // Drive into it, hard, for long enough to be well and truly stopped.
    let to_wall = drive(
        &mut d,
        &w,
        &b,
        Drive {
            throttle: 1.0,
            ..Default::default()
        },
        6.0,
    );
    let stopped = d.dir;
    println!("drove {to_wall:.2} m into the wall, at {:.3} m/s", d.speed);
    // Now reverse, which is what a driver does.
    let back = drive(
        &mut d,
        &w,
        &b,
        Drive {
            throttle: -1.0,
            ..Default::default()
        },
        3.0,
    );
    let _ = stopped;
    println!("backed {back:.2} m off it, at {:.3} m/s", d.speed);
    assert!(
        back > 2.0,
        "a car against a wall backed {back:.2} m off it in three seconds"
    );
}

/// A car drives AT PLANET SCALE, which is the radius the game actually
/// runs at and five hundred times the one the rest of this suite uses.
///
/// The scripted drive to the next town went twelve metres and stopped,
/// on open level ground with nothing blocking it: the probe read the
/// ground under the car, two metres ahead of it and two metres behind
/// it as the same 1105.65 m and `resolve_body` moving it nought. Every
/// other test here is on a two kilometre ball.
#[test]
fn a_car_pulls_away_at_planet_scale() {
    for radius in [2_000.0, 100_000.0, 1_000_000.0] {
        let b = Bounds {
            radius,
            floor: radius - 50.0,
            top: radius + 50.0,
            sea: 0.0,
        };
        let ground = Sphere { radius };
        let w = Built {
            ground: &ground,
            blocks: vec![],
        };
        let mut d = Driver::board(&w, &b, DVec3::Y, DVec3::X);
        let from = d.dir;
        for _ in 0..600 {
            d.update(
                &w,
                &b,
                &Drive {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        let gone = from.angle_between(d.dir) * radius;
        println!(
            "on a {radius:.0} m ball: {gone:.1} m in ten seconds at {:.1} m/s",
            d.speed
        );
        assert!(
            gone > 100.0,
            "a car on a {radius:.0} m ball went {gone:.1} m in ten seconds"
        );
    }
}

/// A car that has been SLOWED by a crash can pull away again.
///
/// `roll` refuses a step that made under a twentieth of what it asked
/// for, which is the right shape and the wrong SCALE: the threshold is
/// a share of the asked step, and the asked step is the speed, so a car
/// knocked down to a crawl asks for a step of a millimetre and any
/// rounding in the push out fails it. The crash then knocks it down
/// again. It is a LATCH: nothing the throttle does can ever get out of
/// it, in either gear, and that is a car dead on open ground.
#[test]
fn a_car_slowed_to_a_crawl_can_pull_away_again() {
    let b = bounds();
    let w = world(&[]);
    let mut d = car(&w, &b);
    // Put it where a crash leaves it: a hair of speed on open ground.
    d.speed = 0.01;
    let gone = drive(
        &mut d,
        &w,
        &b,
        Drive {
            throttle: 1.0,
            ..Default::default()
        },
        5.0,
    );
    println!(
        "pulled away {gone:.1} m in five seconds, reaching {:.1} m/s",
        d.speed
    );
    assert!(
        gone > 30.0,
        "a car at a crawl on open ground went {gone:.1} m in five seconds"
    );
}
