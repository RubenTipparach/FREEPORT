# Design

## Context

`drive::chase` places the camera from the car's pose and the eased swing heading, with `STRETCH` and `RISE` at speed.

## Goals / Non-Goals

**Goals:**
- A camera that is never inside a box a body collides with.

**Non-Goals:**
- Changing the swing or the stretch.

## Decisions

- March the line from the car's roof to the wanted camera place against the car's `underfoot` field and stop short of the first solid, eased so the camera does not snap.

## Risks / Trade-offs

- Trim (eaves, parapets) is not collided and can still sit between the camera and the car; it is thin and passes through quickly.
