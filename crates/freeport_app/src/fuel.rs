//! FUEL on the harness: the player's purse, the jerrycan in hand, the
//! gauge on the dash, and G at a pump.
//!
//! The rules are the core's (`freeport_core::fuel`): eighty kilometres
//! a tank, ten dollars a fill, a thousand to start. What is here is the
//! one key and the two pieces of state the core cannot hold, which is
//! how much the PLAYER has and whether he is carrying a can.
//!
//! **G** is gas, one key for the three things a player does with it,
//! because they are one verb and a player looks for one button: at the
//! wheel beside a pump it fills the car; on foot beside a pump it buys
//! a jerrycan; on foot beside a car the player owns, with a can in
//! hand, it pours the can in. The fly camera's own G (face the target
//! body) is in the air, which is neither.

use crate::drive::{nearest_parked, Thefts};
use crate::roads::Network;
use crate::walk::OnFoot;
use crate::Status;
use bevy::prelude::*;
use freeport_core::driver::{self, Driver};
use freeport_core::fuel::{buy_can, buy_tank, Bought, Purse, PUMP_REACH, STOPPED, TANK_PRICE};

/// What the player has to pay with.
#[derive(Resource, Default)]
pub struct Wallet(pub Purse);

/// Whether the player is carrying a full tank of fuel on foot.
#[derive(Resource, Default)]
pub struct Jerrycan(pub bool);

/// How far a body on foot may stand from a pump to buy a can at it,
/// metres: a stride or two further than a car, because the forecourt is
/// walked across and not pulled up on.
const FOOT_REACH: f64 = PUMP_REACH * 1.5;

/// The gauge on the dash: what is in the tank and how far it goes, what
/// is in the purse, and how far the next pump is.
pub fn gauge(car: &Driver, wallet: &Wallet, roads: &Network) -> String {
    let next = roads
        .nearest_pump(car.dir * car.foot)
        .map_or(String::new(), |(d, _)| {
            format!(", next gas {:.1} km", d / 1000.0)
        });
    format!(
        "fuel {:.0}% ({:.0} km){}, ${}{next}",
        car.tank.0 * 100.0,
        car.tank.reach() / 1000.0,
        if car.tank.empty() {
            ", DRY: walk to a pump for a can"
        } else {
            ""
        },
        wallet.0.dollars,
    )
}

/// What the purse and the can add to the line a walker reads.
pub fn show_purse(
    walker: Option<Res<OnFoot>>,
    wallet: Res<Wallet>,
    can: Res<Jerrycan>,
    mut status: ResMut<Status>,
) {
    if walker.is_none() {
        return;
    }
    let held = if can.0 { ", a jerrycan in hand" } else { "" };
    status.walker = format!("{} | ${}{held}", status.walker, wallet.0.dollars);
}

/// G: fill the car at a pump, buy a jerrycan at one on foot, or pour the
/// can into a car the player owns.
pub fn refuel(
    keys: Res<ButtonInput<KeyCode>>,
    walker: Option<Res<OnFoot>>,
    roads: Res<Network>,
    mut thefts: ResMut<Thefts>,
    mut wallet: ResMut<Wallet>,
    mut can: ResMut<Jerrycan>,
    mut status: ResMut<Status>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let said = match (thefts.at_wheel, walker) {
        (Some(k), _) => at_pump(&mut thefts.cars[k].car, &roads, &mut wallet),
        (None, Some(w)) => on_foot(
            w.0.dir * w.0.foot,
            &roads,
            &mut thefts,
            &mut wallet,
            &mut can,
        ),
        (None, None) => return,
    };
    info!("{said}");
    status.walker = said;
}

/// The car is at the wheel: fill it if it is standing at a pump.
fn at_pump(car: &mut Driver, roads: &Network, wallet: &mut Wallet) -> String {
    if car.speed.abs() > STOPPED {
        return "stop at the pumps to fill up".to_string();
    }
    match roads.nearest_pump(car.dir * car.foot) {
        Some((d, _)) if d <= PUMP_REACH => match buy_tank(&mut wallet.0, &mut car.tank) {
            Bought::Filled => format!("filled up for ${TANK_PRICE}, ${} left", wallet.0.dollars),
            Bought::AlreadyFull => "the tank is already full".to_string(),
            Bought::Short => format!(
                "a tank is ${TANK_PRICE} and the purse holds ${}",
                wallet.0.dollars
            ),
        },
        Some((d, _)) => format!("no pump within reach: the nearest is {:.0} m off", d),
        None => "no gas stations on this body".to_string(),
    }
}

/// On foot: pour the can into a car within reach, else buy one at a
/// pump within reach.
fn on_foot(
    here: bevy::math::DVec3,
    roads: &Network,
    thefts: &mut Thefts,
    wallet: &mut Wallet,
    can: &mut Jerrycan,
) -> String {
    if can.0 {
        return match nearest_parked(thefts, here) {
            Some(i) => {
                thefts.cars[i].car.tank.fill();
                can.0 = false;
                "poured the can into the car: a full tank".to_string()
            }
            None => format!(
                "carrying a jerrycan and no car of yours within {:.0} m",
                driver::REACH
            ),
        };
    }
    match roads.nearest_pump(here) {
        Some((d, _)) if d <= FOOT_REACH => {
            if buy_can(&mut wallet.0) {
                can.0 = true;
                format!(
                    "bought a jerrycan for ${TANK_PRICE}, ${} left",
                    wallet.0.dollars
                )
            } else {
                format!(
                    "a can is ${TANK_PRICE} and the purse holds ${}",
                    wallet.0.dollars
                )
            }
        }
        Some((d, _)) => format!("no pump within reach: the nearest is {:.0} m off", d),
        None => "no gas stations on this body".to_string(),
    }
}
