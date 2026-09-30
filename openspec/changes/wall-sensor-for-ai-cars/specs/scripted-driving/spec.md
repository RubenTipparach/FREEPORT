# Spec Delta

## Purpose

A car driven by a script sees the walls round it and never drives into one.

## ADDED Requirements

### Requirement: A scripted car never meets a wall
A scripted car SHALL slow for a wall in its path and steer away from one closing on either side, so that it never touches a building's box.

#### Scenario: The trip errand
- **WHEN** the bot's `trip` errand runs from the port to the next town
- **THEN** the car never overlaps a building's box, and the report counts no wall contacts
