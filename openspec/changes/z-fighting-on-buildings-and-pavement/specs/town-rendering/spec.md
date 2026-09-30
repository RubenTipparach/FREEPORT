# Spec Delta

## Purpose

A town's buildings, streets and pavements draw without two surfaces fighting for the same pixels, at any distance a player sees them from.

## ADDED Requirements

### Requirement: No two drawn faces of a town lie in the same plane where they overlap
Every pair of a town's faces that overlap on screen SHALL be separated by at least what the depth buffer resolves at the farthest distance that detail is drawn, or one of the two SHALL not be drawn.

#### Scenario: A walk and a drive through the port
- **WHEN** the bot walks and drives the port with pictures every second
- **THEN** no picture shows flicker on a wall, a trim, a kerb or a pavement join, and a pair of pictures one frame apart differ there by less than the scene's own noise floor (`tools/pngdiff.py`)
