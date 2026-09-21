//! FUEL and MONEY: what a car burns, what a tank costs, and what the
//! player has to pay for it with.
//!
//! The owner's numbers, all of them: a thousand dollars to start, a car
//! that goes about eighty kilometres on a tank, a full tank for ten
//! dollars at a station on the highway, and a jerrycan a player on foot
//! can carry one tank of back to a car that ran dry. It is in the core
//! because it is the one economy this world has so far, and what a
//! cargo is worth is the core's by this project's own rule: if two
//! clients computed it differently the world would diverge.
//!
//! A tank is a SHARE and never litres, because nothing here has a litre
//! in it: what a car has is a distance, and eighty kilometres is the
//! whole of what the owner said about it.

/// How far a full tank goes, metres. A car burns the ground it MAKES
/// (`driver::Driver::gone`), so a car wedged against a wall with its
/// wheels turning burns nothing, and a car coasting down a mountain
/// burns for every metre of it.
pub const RANGE: f64 = 80_000.0;
/// What a full tank costs at a pump, dollars, and what a player starts
/// with.
pub const TANK_PRICE: u32 = 10;
pub const START_CASH: u32 = 1_000;
/// How near a car has to be to a station's pumps to be filled, metres
/// from the forecourt's own middle, and how slowly it has to be going:
/// "pull up to a gas station" is a car that has stopped beside one.
pub const PUMP_REACH: f64 = 14.0;
pub const STOPPED: f64 = 1.0;
/// How much of a tank a car taken off the street has in it, at the
/// least and at the most: nobody parks with a full tank, and a car
/// stolen dry is a walk.
const LEAST: f64 = 0.30;
const MOST: f64 = 0.90;

/// A car's TANK: how much of a full one is in it, one down to nought.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tank(pub f64);

impl Tank {
    /// A full tank.
    pub fn full() -> Tank {
        Tank(1.0)
    }

    /// The tank a car taken off the street happens to have, off its own
    /// dice: between `LEAST` and `MOST` of a full one.
    pub fn part(dice: f64) -> Tank {
        Tank(LEAST + (MOST - LEAST) * dice.clamp(0.0, 1.0))
    }

    /// Whether there is nothing left to burn: under a metre's worth,
    /// because eight hundred burns of a hundred metres leave a rounding
    /// and a rounding is not fuel.
    pub fn empty(&self) -> bool {
        self.0 * RANGE < 1.0
    }

    /// Burn the fuel `metres` of ground costs, down to nought and never
    /// past it. A NaN or a negative distance burns nothing: a tank is
    /// never refilled by a body that went backwards.
    pub fn burn(&mut self, metres: f64) {
        if metres.is_finite() && metres > 0.0 {
            self.0 = (self.0 - metres / RANGE).max(0.0);
        }
    }

    /// Fill it.
    pub fn fill(&mut self) {
        self.0 = 1.0;
    }

    /// How far what is left goes, metres.
    pub fn reach(&self) -> f64 {
        self.0.max(0.0) * RANGE
    }
}

/// The player's PURSE, whole dollars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Purse {
    pub dollars: u32,
}

impl Default for Purse {
    fn default() -> Self {
        Purse {
            dollars: START_CASH,
        }
    }
}

impl Purse {
    /// Pay `price` if it is there, and say whether it was.
    pub fn pay(&mut self, price: u32) -> bool {
        if self.dollars < price {
            return false;
        }
        self.dollars -= price;
        true
    }
}

/// What a player can buy at a pump: a tank into a car standing there, or
/// a jerrycan to carry away. One price, because a jerrycan IS a tank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bought {
    /// The tank was filled and the price paid.
    Filled,
    /// The tank was already full, so nothing was sold.
    AlreadyFull,
    /// Not enough in the purse.
    Short,
}

/// Fill `tank` at a pump out of `purse`.
pub fn buy_tank(purse: &mut Purse, tank: &mut Tank) -> Bought {
    if tank.0 >= 1.0 {
        return Bought::AlreadyFull;
    }
    if !purse.pay(TANK_PRICE) {
        return Bought::Short;
    }
    tank.fill();
    Bought::Filled
}

/// Buy a JERRYCAN out of `purse`, which is a full tank carried on foot:
/// true if it was paid for.
pub fn buy_can(purse: &mut Purse) -> bool {
    purse.pay(TANK_PRICE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tank_goes_eighty_kilometres_and_then_the_car_is_dry() {
        let mut tank = Tank::full();
        for _ in 0..799 {
            tank.burn(100.0);
            assert!(
                !tank.empty(),
                "dry at {:.1} km",
                80.0 - tank.reach() / 1000.0
            );
        }
        tank.burn(100.0);
        assert!(tank.empty());
        assert!(tank.reach() < 1.0);
        // Nothing past nought, and nothing for a NaN or for reversing.
        tank.burn(1e9);
        tank.burn(f64::NAN);
        tank.burn(-5.0);
        assert_eq!(tank, Tank(0.0));
        let part = Tank::part(0.5);
        assert!(part.0 > LEAST && part.0 < MOST && !part.empty());
        assert!((Tank::part(-3.0).0 - LEAST).abs() < 1e-12);
        assert!((Tank::part(7.0).0 - MOST).abs() < 1e-12);
    }

    #[test]
    fn a_full_tank_is_ten_dollars_out_of_a_thousand_and_a_can_is_the_same() {
        let mut purse = Purse::default();
        assert_eq!(purse.dollars, 1000);
        let mut tank = Tank(0.2);
        assert_eq!(buy_tank(&mut purse, &mut tank), Bought::Filled);
        assert_eq!(purse.dollars, 990);
        assert_eq!(tank, Tank::full());
        assert_eq!(buy_tank(&mut purse, &mut tank), Bought::AlreadyFull);
        assert_eq!(purse.dollars, 990);
        assert!(buy_can(&mut purse));
        assert_eq!(purse.dollars, 980);
        // Ninety eight more cans and the purse is empty, then nothing
        // sells.
        for _ in 0..98 {
            assert!(buy_can(&mut purse));
        }
        assert_eq!(purse.dollars, 0);
        assert!(!buy_can(&mut purse));
        let mut dry = Tank(0.0);
        assert_eq!(buy_tank(&mut purse, &mut dry), Bought::Short);
        assert!(dry.empty());
    }
}
