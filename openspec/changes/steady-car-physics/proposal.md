# Proposal

## Why

The owner saw cars "keep flipping around even when small hits happen" and "getting in a car causes them to shake violently". The ram log shows spins of 2 to 4.8 rad/s from knocks at a few metres a second.

## What Changes

- Measure a knock's spin against its closing speed and bring small knocks down to a small turn.
- Find what shakes a car on boarding (the ground under a car taken off the rails, the kerb, the collider of the car it was, or the camera) and stop it.

## Capabilities

### New Capabilities
- `car-physics`: how a car answers a knock and a boarding.

### Modified Capabilities

## Impact

`freeport_core::ram`, `freeport_core::driver`, `crates/freeport_app/src/drive.rs`.
