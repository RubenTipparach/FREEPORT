# Proposal

## Why

The owner saw the chase camera clipping behind building walls, and the bot's own pictures show it: a frame of nothing but brick while the car is beside a building in a narrow street. The camera sits 8.5 m behind the car and further at speed, and nothing stops it inside a wall.

## What Changes

- Pull the camera in along its line to the car when a wall stands between them, using the same boxes the car collides with, and ease it back out.

## Capabilities

### New Capabilities
- `chase-camera`: where the camera behind a driven car may stand.

### Modified Capabilities

## Impact

`crates/freeport_app/src/drive.rs` (`chase`, `look_at`), reading `Fabric::underfoot`.
