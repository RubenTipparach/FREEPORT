//! freeport_core: what the game KNOWS, with no engine in it.
//!
//! Everything here is a rule two clients would have to agree on or a
//! measurement a test can hold still: where a thing is in metres, which patch
//! of a planet is near enough to draw at what detail, what the ground is made
//! of at a point, and the triangles that ground turns into. Bevy draws what
//! this crate says and never decides any of it. The test for what belongs
//! here is swarm-demo's: if two clients computed this differently, would the
//! world diverge? Then it lives in the core.

pub mod atmos;
pub mod audit;
pub mod biome;
pub mod chart;
pub mod dc;
pub mod field;
pub mod flight;
pub mod lattice;
pub mod march;
pub mod model;
pub mod noise;
pub mod pos;
pub mod qef;
pub mod road;
pub mod sphere;
pub mod tables;
pub mod town;
pub mod walker;
pub mod water;
