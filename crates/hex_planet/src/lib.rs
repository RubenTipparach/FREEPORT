//! Experimental ten kilometre planet renderer core.
//!
//! This crate is deliberately separate from `freeport_core`. It tests the
//! alternative Tenebris-style route: a Goldberg surface near the observer,
//! a continuous displaced sphere farther away, and one atmosphere model used
//! both for the visible sky and ambient light.

pub mod atmosphere;
pub mod city;
pub mod hex;
pub mod lod;

/// Radius of the experimental planet in metres.
pub const PLANET_RADIUS: f64 = 5_000.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planet_is_ten_kilometres_across() {
        assert_eq!(PLANET_RADIUS * 2.0, 10_000.0);
    }
}
