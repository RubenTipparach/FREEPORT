# Civil engineering: what a road in this world is built to

This is the reference `CLAUDE.md` sends any road generation question to.
It is the real discipline's own numbers, the formula each comes out of,
and then what FREEPORT picks and why, so a constant in `road.rs` or
`road/ribbon.rs` can be checked against something other than taste.

Where this world deliberately departs from the standards, the departure
is named here rather than hidden. A generated planet is not a highway
authority and some of its numbers are the game's; what is not allowed is
a number nobody can trace.

## The one input everything else falls out of: DESIGN SPEED

Every geometric standard below is a function of the speed the road is
built for. Pick that first and the rest is arithmetic.

FREEPORT's design speed is `driver::TOP`, **44.4 m/s (160 km/h)**, and
that is not a choice about roads, it is a fact about the cars: it is the
fastest thing this world puts on one, so it is what the alignment has to
hold. `road::DESIGN_SPEED` reads that constant rather than repeating it,
which is this project's own rule that two places that need one number
call one function.

For scale, real practice: 50 km/h urban street, 80 to 100 km/h rural two
lane, 110 to 130 km/h motorway. 160 km/h is past what any authority
designs for, so this world's roads are built to a standard a little
above a motorway's, which is the honest consequence of a car that does
160.

## Horizontal curves

**The formula.** A vehicle on a curve is held by the bank of the road
and by the tyres:

```
R = V^2 / (127 (e + f))        V in km/h, R in metres
R = v^2 / (g (e + f))          v in m/s, R in metres
```

The 127 is `3.6^2 * 9.81`, so the two are one formula.

**Superelevation `e`**, the bank. Practice caps it by climate and by
what a slow vehicle can stand on:

| e_max | where |
| --- | --- |
| 4% | urban, or anywhere ice and stopped traffic are expected |
| 6% | the common general case |
| 8% | rural, free of ice |
| 10 to 12% | mountainous, rare, and hard on slow vehicles |

**Side friction `f`**, what is left to the tyres. It FALLS with speed,
because the factor is set by comfort rather than by grip: 0.17 at
30 km/h, 0.14 at 60, 0.12 at 100, 0.11 at 110, 0.09 at 120, 0.08 at
130.

**The minimum radius at 60 mph (about 97 km/h)**, which is the figure
the owner quoted: about 560 ft at the tightest published design radius
and over 1,400 ft where the banking is low, with 1,200 to 1,340 ft the
usual answer at e = 8%. Through the formula at 60 mph exactly
(26.822 m/s, 96.56 km/h) with e = 0.08 and f = 0.12: **R = 367 m
(1,204 ft)**, which is 366.7 m through the SI form and 367.1 through
AASHTO's 127, the two differing because 127 is itself a rounding of
3.6^2 g = 127.14. The spread in the quoted range is the spread in `e`
and in which authority's `f` table is used, and both ends of it are
real.

**And the minimum is a LIMIT rather than a target.** Every design guide
says the same thing in the same place: use the largest radius the
ground will give you. The minimum is what a curve may not be tighter
than, not what a curve should be.

**What FREEPORT builds.** `road::CURVE` is the formula at the design
speed with e = 8% and f = 0.10:

```
R = 44.4^2 / (9.81 * 0.18) = 1,116 m
```

so a bend in a highway here is a circular arc of at least 1,116 m
radius. `road::centreline` fits one at every vertex of the routed
polyline, and the fit is PURE GEOMETRY derived on both sides of the
bake rather than stored, which is the same rule `road::pieces` and
`road::step` already keep.

**Two things cap that radius, and both are named where they bind.** A
curve's tangent length is `T = R tan(delta / 2)` and it cannot eat more
than its share of the legs either side of it, or two neighbouring
curves overlap; and a curve cuts the corner by its mid ordinate
`M = R (sec(delta / 2) - 1)`, which is how far the built road leaves
the line the router chose over real ground. `road::CURVE_SHARE` and
`road::CURVE_OFFSET` are those two, and where either binds the curve is
built at the largest radius that fits rather than skipped.

Measured, on ten kilometre legs: a bend of 10, 30, 45 or 60 degrees is
built at the full 1,116 m, and a right angle at **604 m**, where the
corner cutting cap wins. 604 m through the same formula is 118 km/h, so
that is a bend a real road would sign rather than one nobody can take,
and it is still twenty times the 60 m the unaligned corner amounted to
at the spacing the road is laid in.

## Transitions, which this world does NOT build

Real alignments put a spiral (a clothoid) between the straight and the
arc, so the steering wheel turns at a finite rate and the
superelevation has somewhere to run out. FREEPORT joins the tangent to
the arc directly.

The reason is that the joint is under the resolution anything here can
see: an arc of 1,116 m is walked in `road::PIECE` chords of 85 m, so
the heading steps 4.4 degrees a chord anyway, and a spiral would be
smoothing a curve the tarmac already draws as a polygon. It is named
here so the next reader knows it was a decision.

## Grades

| road | max grade |
| --- | --- |
| motorway, level terrain | 3 to 4% |
| rural arterial, rolling | 5 to 6% |
| rural arterial, mountainous | 7 to 8% |
| local and mountain roads | 9 to 12% |

**FREEPORT uses 10%** (`road::STEEPEST`, one in ten), which is a
mountain road rather than a motorway. That is a deliberate departure
and the reason is the body: the relief is 8,000 m over a 1,000 km
radius and the settlements are placed by the ground, so a network held
to 4% would refuse most of the edges it needs and leave the interior of
every continent unjoined. The routing cost function already leans hard
against climbing (`road::GRADE`, eight times the flat distance per unit
of grade), so a route takes the long way round rather than the steep
way over wherever it can.

## The cross section

| part | real practice | FREEPORT |
| --- | --- | --- |
| lane | 3.0 to 3.7 m | 2.75 m (`town::LANE`) |
| shoulder, rural two lane | 1.2 to 3.0 m | 0.7 m (`ribbon::SHOULDER`) |
| verge and graded formation | the shoulder plus the batter's top | 16 m either side (`road::CORRIDOR`) |
| surfacing over the base | 0.1 to 0.3 m | 0.15 m (`ribbon::LIFT`) |
| crossfall for drainage | 2% | not built |

The lane is narrow for a highway and is the town street's own lane,
read off one constant so a car that fits a street fits a road. The
graded formation is wide for the traffic it carries and the reason is
the renderer rather than the traffic: the corridor's flat is what
carries the road through the terrain's own LOD, and that is written up
at `road::CORRIDOR`.

## Embankments and cuttings

An alignment that only ever followed the ground would be unbuildable
and unusable, so a road is built on CUT and FILL: the formation is
carried at its own grade and the country is taken away under it or
built up to it.

**Side slopes (batters).** Practice, as vertical to horizontal:

| slope | where |
| --- | --- |
| 1:2 | the steepest a fill is normally built at |
| 1:3 | preferred where there is room: traversable, so a car leaving the road can recover |
| 1:4 to 1:6 | flat enough to mow and to drive down, used on low fills beside fast roads |
| 1:1.5 or steeper | rock cuttings, or a fill with a retaining structure |

The number that matters for a game is the **traversable** one: a 1:3
batter is the slope a vehicle running off the carriageway can come back
up, and it is the slope a walker can climb without being stopped.

**The MOUND this game draws.** `road::EMBANK` is 2 m, so the formation
stands two metres out of the country everywhere and the corridor fills
the ground up to it. `ribbon::mound` then DRAWS that: the verge out to
`road::CORRIDOR` and the batter down the skirt, every point of it read
off the field's own surface, so the embankment that is drawn and the
embankment a walker climbs are one surface rather than two that have to
agree.

**What FREEPORT builds.** The corridor's own skirt
(`field::SKIRT_IN + SKIRT_OUT`, 11 m) carries the fill down to the
natural ground, so the batter's slope is whatever that fall asks for:
at `road::EMBANK` it is about 1:5, and at the deepest fill on this body
(18 m over a hollow) it is about 1:0.6, which is a rock face. The
SLOPE is therefore not a constant here, it is a consequence, and the
one thing the game holds is that the drawn batter is the same surface
the walker and the car collide with (`road/ribbon.rs`).

**And a road only ever RISES.** `road::smooth` raises its profile to
clear the ground and never lowers it into a cutting, which is a
departure from practice (a real alignment balances cut against fill to
avoid hauling material). The reason is the renderer and it is written
up in that function: a cutting is 30 m wide and the terrain's cell past
a couple of kilometres is wider than that, so a cutting the mesher
cannot resolve closes over the road, while a fill it cannot resolve
merely leaves the road standing proud of the ground.

## Junctions, where two alignments share one formation

Every road on this body comes off one Dijkstra tree, so the roads out
of a town run on the same chain of waypoints until their routes split
and are laid over one another for kilometres. Practice has no such
thing as two carriageways on one formation: where two alignments meet
they are ONE road, graded to one profile, and a road that leaves it
does so on a ramp at the design grade. `road::trunk` is that rule at
load, and its numbers:

| part | real practice | FREEPORT |
| --- | --- | --- |
| two alignments are one road when | their carriageways overlap | centrelines within 4 m (`trunk::SHARE`), a lane and a half, since a carriageway is two lanes of 2.75 m |
| a crossing is not a merge when | the overlap is a junction's own length | fewer than 3 stations, 170 m (`trunk::LEAST`) |
| the shared profile | the higher alignment's, filled up to, never cut down to | the highest of the roads on it, every road raised to it at the grade (`trunk::merge`) |
| the merging road's tarmac over the trunk's | a surfacing overlay, 25 to 50 mm | 30 mm (`trunk::FORK_LIFT`) |
| the ramp off a trunk | the design grade, 7% here | `road::STEEPEST`, held by the same envelope `smooth` bakes with |

The shared profile is the HIGHER one and not the owner's, for the reason
the grade section gives: a road only ever rises, because a cutting is
the one thing this terrain cannot draw at distance. A road pinned down
to a lower trunk has to climb back to its own baked profile past the
fork, and its own profile was raised to hold the grade toward its own
high ground, so the whole climb landed on the one piece past the fork:
measured, 103% on the steepest fork on the body. Raising the trunk to
the higher road and easing every road off it at the grade puts nothing
over seven per cent anywhere, and the price is embankment on the lower
road through the junction, which is what a real merge is built on.

## Sight distance and clear zones, which this world does not model

Real geometry is driven as much by what a driver can SEE as by what a
car can hold: stopping sight distance sets the minimum vertical curve
and the offset to an obstruction inside a horizontal curve, and the
clear zone sets how far from the carriageway a solid object may stand.
None of that is modelled here. The lamp standards stand 0.5 m off the
carriageway (`ribbon::LAMP_OUT`), which is inside any real clear zone,
and they are drawn rather than collided precisely because nothing here
crashes into them.

## The checklist for a road generation change

1. Which of the numbers above does it move, and is the new value in
   the table or a named departure from it?
2. Is it derived from the design speed, or is it a constant with a
   reason beside it?
3. Does it move the atlas's own fingerprint (`road::PIECE`,
   `road::EMBANK`, `road::CURVE`)? Then the atlas is re-baked in the
   same change and `Atlas::fits` refuses the old one.
4. Is the thing the player collides with the same surface the thing
   the player sees?

## Sources

The formulas and the ranges above are AASHTO's *A Policy on Geometric
Design of Highways and Streets* as they appear in every state design
guide; the 60 mph radius range is the one the owner quoted from the
Massachusetts Project Development and Design Guide, chapter 4
(horizontal and vertical alignment). Nothing here is a number this
project invented; what this project chose is in the FREEPORT column
and says why.
