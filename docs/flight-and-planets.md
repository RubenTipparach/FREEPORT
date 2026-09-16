# Flight and planet exploration

Freeport now has three nearby landable worlds: Ember (420 km radius), Pelagos
(760 km), and Rime (280 km). Their positions, relief, sea levels and seeds come
from `assets/config/planets.json`. The worlds are stationary for now.

In fly mode, scroll up/down to multiply/divide cruise speed by 1.6. The range is
0.25 m/s to 2,000,000 m/s, with Shift multiplying the request by eight. N cycles
destinations and G points the camera at the selected centre. Use W to travel,
Q/E to bank, Space/Ctrl for local vertical motion, R to level against the nearest
planet, and F to walk. These actions retain the selected cruise speed.

`assets/config/flight.json` adds `surface_speed` (6 m/s) and `ground_clearance`
(0.5 density units). Existing speed, boost, roll, bounds and wheel settings still
apply; zero selects the default. The HUD displays actual motion separately from
cruise speed, plus the nearest body, mean-radius altitude and target distance.

The reference is Tenebris C's
[player controller](https://github.com/RubenTipparach/tenebris/blob/main/tenebris-c/src/client/player_ctrl.c):
local 6DOF controls and body-relative positions are retained. Freeport uses a
continuous altitude speed envelope rather than switching to Tenebris's walking
controller on atmosphere entry. The envelope uses smoothstep from surface speed
to requested cruise speed, and applies after boost. Movement is split at the
atmosphere boundary and again by altitude, so a long frame cannot skip the region.
Lower wheel settings remain lower; departure restores cruise speed automatically.

Slowing down is separate from collision. `freeport_core::flight::sweep` checks the
entire segment using the terrain density's slope bound. It catches a trajectory
whose endpoints are both outside a planet, and stops at the last verified clear
point if its work budget runs out. Collision uses the procedural field even when
terrain meshes are still loading. Contact recovery leaves room to take off again;
invalid underground starting positions are moved outside. This protects terrain;
fly mode can still pass through buildings and water. Walking keeps its existing
building and water collision.

The active body's terrain, water, walking coordinates and atmosphere use its
local frame. Rendering anchors add its f64 centre before floating-origin rebasing.
The streamer retains its compute workers across switches, cancels old jobs, and
tags results with a generation so a late old-body result cannot replace new
terrain. GPU batches are partitioned by world before sampling.

Whole-body distant views currently use coarse, coloured interior spheres below
the lowest terrain. Detailed ground and water stream around the player on every
body. These distant views are approximations, not the adaptive Planet-LOD surface
or a welded surface/volume transition. The existing L/K diagnostic still inspects
the actual volumetric terrain; the broader renderer work in
[the Planet-LOD review](planet-lod-review.md) remains separate.

Regression checks cover boosted entry at 10/30/60/144 FPS, rough density fields,
whole-planet tunnelling, landing and takeoff, translated bodies, walking/flight
handoff, wheel input, and rejection of stale chunks after a body switch.
