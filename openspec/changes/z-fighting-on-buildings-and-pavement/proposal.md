# Proposal

## Why

The owner saw "a lot of z-fighting on the buildings and pavement, and various parts of the building", and asked whether the floating origin was implemented well. It was checked: the town is one entity placed through the origin with its tiles in the town's own frame, the farthest vertex in the 1,815 m port is about 900 m from that frame's centre where an f32 holds about 0.06 mm, and the depth buffer is infinite reverse Z at 32 bits. Precision does not explain a flicker an eye can see, so the faces are really coincident.

## What Changes

- Find the coincident faces with close pictures (`--bot-shots` and a solved camera): trim laid flat on walls, pavement slabs overlapping at the joins between street pieces and crossings, two building detail levels drawn in the same frame, and shadow acne mistaken for z-fighting.
- Separate every coincident pair by a measured amount, or remove the duplicate.

## Capabilities

### New Capabilities
- `town-rendering`: a town's buildings and paving draw without coincident faces.

### Modified Capabilities

## Impact

`freeport_core::model` (trim, solids, street pieces), `crates/freeport_app/src/city/*` (tile levels of detail), possibly the shadow bias in `main.rs`.
