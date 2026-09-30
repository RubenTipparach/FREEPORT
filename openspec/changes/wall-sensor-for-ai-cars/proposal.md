# Proposal

## Why

The owner asked that AI cars, including the one the bot drives, "should have some kind of sensor so they dont just drive into buildings". The bot's drive no longer hits walls on the port's route, but only because its steering stopped cutting corners; nothing in it can see a wall.

## What Changes

- A short look ahead along the car's path against the same boxes the car collides with: slow for a wall ahead, and steer away from the side a wall is closing on.
- The same sensor keeps a car off a wreck standing in the street, which today it only goes round after four seconds held up and a back off.

## Capabilities

### New Capabilities
- `scripted-driving`: what a car driven by a script sees and keeps off.

### Modified Capabilities

## Impact

`crates/freeport_app/src/drive/script.rs` (`Auto`), reading the car's `underfoot` field.
