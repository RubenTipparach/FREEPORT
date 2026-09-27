# Design

## Context

Built things are `Model::solid` boxes and `Model::trim` quads in the town's frame; paving is pieces per run and crossing; tiles swap between baked levels of detail and district proxies.

## Goals / Non-Goals

**Goals:**
- Name every coincident pair with a picture and a measurement.

**Non-Goals:**
- Any change to the floating origin, which was measured and is sound.

## Decisions

- Diagnose first: pictures at close range, and a two frame pixel diff on a still camera, which separates z-fighting (changes frame to frame) from aliasing (does not).
- Fix by separation measured against the depth step at the draw distance, or by culling the duplicate.

## Risks / Trade-offs

- An offset that is too large shows as a floating trim; each offset is measured against the depth step rather than guessed.
