# Spec Delta

## Purpose

The camera behind a driven car always shows the car, never the inside of a building.

## ADDED Requirements

### Requirement: The chase camera never stands inside a solid
The chase camera SHALL stand in air with a clear line to the car's roof, moving in along that line when a wall is in the way and back out when it is not.

#### Scenario: A car against a building in a narrow street
- **WHEN** a car is driven along a street with buildings either side and turned toward a wall
- **THEN** every picture shows the car, and no picture is filled by a wall's texture
