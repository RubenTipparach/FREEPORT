# Proposal

## Why

The owner saw cars colliding and not avoiding each other, and suggested that the NPC cars avoid collisions too, "maybe add stop signs to certain roads". A town's cars are closed form functions of the town, their index and the clock, with no state, which is why a town nobody is near costs nothing. They cannot react to anything, but a wait can be written into the function.

## What Changes

- Stop signs at chosen crossings: an agent's place along its loop includes a stop of a few seconds at each signed crossing it passes, still a closed form of the clock.
- Crossing traffic takes turns: which arm goes first at a crossing is a function of the clock, so two closed form cars never meet in one.
- The player's scripted drive already keeps to the clear road ahead against where the rails will have the traffic (`Auto::room`).

## Capabilities

### New Capabilities
- `town-traffic`: when a town's cars stop and go at crossings.

### Modified Capabilities

## Impact

`freeport_core::traffic` (`Circuit::at`, `Traffic::at`), the crossing pieces in `town/street.rs` for the sign's model, and the traffic tests.
