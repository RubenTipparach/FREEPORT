# The bot, and the benchmark that drives it

FREEPORT is about driving a car from one town to another and doing jobs
when you get there, so that is what is measured: not a camera flown along
a rail, but a player. The bot (`crates/freeport_app/src/bot.rs`) is a
player nobody is playing. It walks the streets of the port, walks up to a
car and takes it, and drives the road network to the next town, and it
writes down what every frame of that cost. `tools/bench.py` runs it for
several rounds, on one binary or on two, and turns the rounds into medians
with their spread beside them.

## What the bot does

It is a PLAYER, which is the whole point of it, and each of its phases goes
through the same code a person at the keys does:

| phase | what it does | through |
| --- | --- | --- |
| settle | waits for the ground, the towns and the crowds to come up, and 120 frames more | `Streamer::idle`, `Fabric::settled` |
| walk | walks `--bot-walk` metres (200 by default) of the town's own streets toward its middle, crossing by crossing | the walker's own `Input`, `Streets::route` |
| hail | walks to the nearest car on the rails and takes it once it is within a player's reach | `drive::board`, as though E were pressed |
| drive | drives the route the map plans to the nearest other town, through the town's streets, down the slip and along the highway | the scripted drive (`drive/script.rs`), through the `Autopilot` switch |
| done | gets out once it is within the destination's own radius (60 m at least), or stops when its time runs out | `drive::board` again |

Nothing is teleported. The walker is stopped by the same walls a player
is, so a street the bot cannot get down is a street a player cannot either;
the theft happens at `driver::REACH` (8 m), so the bot has to catch a car;
and the drive is the same pure pursuit along the planned route that
`--drive` has always used. Where it gets stuck is where a player would.

`Autopilot` (`drive/script.rs`) is the one switch the scripted drive asks
whether it is on. `--drive` sets it from the command line and the bot sets
it when it boards, so there is one question and it cannot be on for one of
them and off for the other.

## Running it

```sh
cargo build --release -p freeport_app
./target/release/freeport_app --bot                                # in the window, then the controls are yours
./target/release/freeport_app --bot-report out.json                # measured, and it quits when done
./target/release/freeport_app --bot-report out.json --bot-seconds 120 --bot-walk 100
./target/release/freeport_app --bot-report out.json --bot-fast     # a second of driving a frame
```

| flag | what it does |
| --- | --- |
| `--bot` | run the errand in the window; the frame is held to sixty so it is watched at its own pace |
| `--bot-report PATH` | run it measured, write `PATH` and `PATH` with `.frames.csv`, and quit |
| `--bot-seconds N` | simulated seconds it is given, walking and driving together (1,800) |
| `--bot-walk M` | metres of street it walks before it goes for a car (200) |
| `--bot-fast` | drive a second a frame, the scripted drive's journey pace, for a machine that wants to know the bot ARRIVES and not what the drive's frames cost |

Measured, it keeps the flight benchmark's own rules through
`Args::measuring`: no frame cap, AutoNoVsync, the window drawing while
unfocused, and the keys and the mouse taken away so a person typing in
another window cannot change the run. On top of those it steps the WORLD's
clock a fixed sixtieth a frame (`TimeUpdateStrategy::ManualDuration`), the
same step the walker and the car take. The cars it walks to are then where
they were on the last run, the sun is where it was, and two runs are one
errand. The consequence is the flight benchmark's own: a machine that draws
faster gets through the same errand in less wall time, so the streamer has
less time per metre on it. The report says both the simulated and the wall
seconds.

## What it writes

`out.json` carries:

- `outcome`: `arrived`, `ran out of time`, or `no car came within reach`.
- `frames` and `update_cpu`: a distribution each (samples, p50, p95, p99,
  worst, and how many frames were over 16.7 ms and over 33.3 ms) for `all`
  and for each of `walk`, `hail`, `streets` and `highway`. A frame's `wall`
  time is from one frame's start to the next; its update is the main
  thread's own part of it, `First` to `Last`. `streets` is driving inside
  any town's outline and `highway` is driving outside all of them.
- `goal_town`, `goal_km_at_boarding`, `to_goal_km_at_end`, `walked_m`,
  `driven_m`, `stuck` (how many times it made no ground for three seconds),
  `sim_seconds`, `wall_seconds`, `wall_seconds_by_phase` and `phases` (when
  each phase began, in simulated seconds).
- `settle_seconds`, the streamer's own `terrain` measurement at the end, the
  towns drawn, the GPU, the backend, the resolution, the levels and the cell
  size.

`out.frames.csv` is one line a frame: the frame, what the bot was doing,
the wall and update times, its speed, how far it was from its goal, and
the streamer's loaded and pending chunks. It is what `bench.py spikes`
reads to say what the worst frames were.

## The benchmark

```sh
python tools/bench.py list                             # the errands
python tools/bench.py run                              # town and trip, three rounds each
python tools/bench.py run --scenarios town --rounds 1  # the quick one
python tools/bench.py run --label after --baseline target/bench/<before>
python tools/bench.py ab --base target/bench/base/<old>.exe --head target/release/freeport_app.exe
python tools/bench.py compare target/bench/<before> target/bench/<after>
python tools/bench.py spikes target/bench/<dir>/runs/<run>.frames.csv
```

| errand | what it is |
| --- | --- |
| `town` | walk 200 m of the port, take a car and drive out through its streets: two simulated minutes |
| `trip` | the whole errand, until the bot arrives at the next town or half an hour of simulated time runs out |
| `trip-fast` | the same trip driven a second a frame; not a default, and its drive frame times mean nothing |

Everything lands in `target/bench/<stamp>-<label>/`: `meta.json` (the
machine, the power source and scheme, the source commit and whether it was
dirty, and each binary's hash and whether it is older than the source),
`runs/` (every round's report, CSV and log), `summary.json` and
`report.md`, a table per errand fit for a commit message.

`ab` interleaves the two binaries ABBA, round by round, so a laptop warming
up over twenty minutes charges both sides alike rather than whichever ran
second. A comparison says a change is `better` or `worse` only when it is
bigger than the spread of either side's own rounds, `within noise` when it
is not, and `one round, no spread` when there is nothing to judge it by.
The distances and the simulated seconds are CHECKS rather than scores: two
builds that walk or drive a different distance on the same errand are
running a different errand, and the table says so.

It refuses to run beside a second game or a screen recorder (two games
share one GPU and both their frame times lie, and a recording on a shared
machine is somebody else's measurement), and beside a compiler or a linker
unless `--force`. It says, rather than refuses: a laptop on battery, a CPU
busy before a round, a run that came up on the integrated GPU of a machine
with a discrete one, rounds that ended in different places or differently,
a trip that did not arrive, and a streamer still behind at the end, where a
faster frame can be a frame that streamed less.

## Which system a slow frame was spent on

The bot's report says WHEN a frame was slow and whether the main thread or
the GPU was the slow part (`bench.py spikes`). What names the SYSTEM is a
traced build and `tools/trace.py`:

```sh
CARGO_TARGET_DIR=target/trace cargo build --release -p freeport_app --features bevy/trace_chrome,bevy/debug
target/trace/release/freeport_app --bot-report t.json --bot-seconds 45
python tools/trace.py trace-<stamp>.json --worst 5
```

`bevy/debug` is what puts the systems' own names on their spans. A trace
is big, about 1.5 MB a frame of the port (6.8 GB for 45 simulated seconds
and the settle before them), so `trace.py` streams it a line at a time: a
first pass costs every span and finds the frames, a second opens up the
worst ones, on every thread. Six minutes for that trace on this laptop.
Traced frames are slower than untraced ones, so a traced number finds a
system and an untraced `bench.py` run is what goes in a commit.

## What the first errands found

On an i9-11900H with an RTX 3060 Laptop GPU, the `town` errand's frames are
the MAIN THREAD's: its update is nearly the whole frame at the median and
at the tail. Its worst frames were hitches of 230 to 435 ms, and the trace
named every one of them:

| frame | system | ms |
| --- | --- | ---: |
| the bot takes its car | `drive::board` | 252.5 |
| a car knocked off the rails | `ram::ram_cars` | 262.6 |
| another | `ram::ram_cars` | 240.0 |

Both are `Driver::board`, whose `walker::ground` with no feet known marches
down from the top of the relief band half a metre at a time: kilometres of
air and six thousand samples of an eighteen octave field for every car
taken, knocked off the rails or got out of. `Bounds::near` starts that
march twenty metres over a radius the caller already has, on the long
march's own grid so the ground it finds is the same to the bit.

Measured with `bench.py ab`, three rounds of the `town` errand on each
binary, interleaved ABBA, before `Bounds::near` and after it, both with the
real time steering fixed (the section on the trip below):

| metric | before | after | verdict |
| --- | ---: | ---: | --- |
| frame worst ms | 259 | 41.2 | better, -84.1% |
| frame p95 ms | 24.00 | 23.36 | better, -2.7% |
| frames over 33.3 ms | 50 | 37 | better, -26% |
| frame p50 ms | 13.12 | 13.11 | within noise |
| frame p99 ms | 31.69 | 31.14 | within noise |
| update p50 ms | 12.19 | 12.21 | within noise |
| errand wall s | 105 | 101 | better, -3.4% |
| driven m | 739 | 739 | the same errand, all six rounds |

The hitch is gone and the rest barely moved, which is what a fix to one
march should look like. What is left is the frame itself: 13 ms at the
median and 31 at p99 with the update 12 and 30 of them, so on this laptop
the main thread IS the frame, and the GPU is not what a faster errand
needs. Measured first with the steering broken on both sides, the same fix
read 253 ms to 72.6.

In that first A/B, five of the six rounds ended at the same point to the
millimetre on both binaries. The sixth, a before round, parted after its fourth ram, missed
the fifth and drove 667 m. The likeliest reason, named rather than proved:
the boxes a car meets in a town come from tiles that are built as the eye
moves, under a frame budget, so a frame's timing can decide whether a wall
is there yet. `bench.py` says so whenever it happens (`the route ended
33.9 m apart across rounds`), which is how this was seen at all.

Sphere tracing the march on the field's slope bound was tried first and
measured WORSE, about 400 ms against 250: the bound is isotropic and the
planet's is in the hundreds, while straight down the field changes at about
one a metre, so the trace crawled. And the first cut of `Bounds::near`
started twenty metres up exactly rather than on the grid, found the ground
a few millimetres differently, and ninety seconds of driving later the
A/B was comparing two different errands, 563 m against 478, which the
tool's own distance check said in capitals.

## The trip ARRIVES, and what it took

On the first night the bot's drive could not get the car out of the port.
The bot's own pictures (`--bot-shots`) are what found every one of the
reasons, and each is a rule in the scripted drive now:

- **The real time drive did not steer at all**, and that was this change's
  own plumbing. `drive_car` applied the scripted steering only when a frame
  had more than one sub step, true of `--drive`'s second a frame and false
  of the bot's sixtieth: the car held full throttle dead ahead, left the
  port the wrong way and ran the highway at 160 km/h AWAY from its goal.
  `pedals` says whether the script is driving (`None` for the input) and
  the wheel reads that.
- **It drove down the CENTRELINE.** The town's streets were steered at each
  crossing's own middle, so every car coming the other way was in its path.
  It aims half a lane right of the crossing now (`kept_right`), the middle
  of its own lane, where the town's cars ride.
- **It turned across the CORNER.** The next crossing was steered for from
  halfway down the block, which is a line through the corner building; the
  pictures had the car scraping along one and then buried in its wall. The
  crossing ahead is steered for until the car is in its square or past it.
- **It rammed everything and took the wrecks for walls.** Every car it
  knocked off the rails stayed in the street as a box. It keeps its speed to
  the clear road in its own lane (`Auto::room`), against where the cars are
  and where the rails will have them over the next two seconds, which is the
  same closed form asked a little later, so a car about to cross the
  junction ahead is in the way before it gets there. Held up is not wedged
  for four seconds, and a car that has not moved by then is one it goes
  round.
- **50 km/h on a town's streets** (`TOWN_SPEED`), 160 on the highway.
- **It drove on past the town.** Arrival was the town's nominal radius and
  the car came within 267 m of a village's middle, inside the village, and
  carried on. It is the town's own OUTLINE now, and there the bot pulls up
  (`Autopilot::park`), stops, and gets out.

Measured, the whole `trip` errand in real time: a 200 m walk, a car taken
at 25.6 s, 1.7 km of the port's streets, the highway at 160 km/h, and
**arrived** in town 160 at 471 s, 13.0 km driven, pulled up 303 m from the
village's middle. The frames: p50 14.5 ms, p99 23.9 ms, worst 46.8 ms.

What is still wrong on the way, named rather than hidden: the cars on the
rails still drive into it, because they are a closed form and cannot know
it is there (nine knocks on the way out of the port, none at speed); and
at 160 km/h the streamed terrain falls behind the road.

## What is missing, named rather than hidden

- The missions. Washing dishes, serving food, cooking, deliveries and rides
  are what a town is for, and none of them exists yet. Each becomes an
  errand here the day it does: a delivery is a pickup and a drop off on the
  same drive, and a ride is a walk to a car with somebody in it.
- The bot walks a street's middle rather than its pavement, which is the
  one line between two crossings nothing stands on. It crosses kerbs only
  where it walks straight at a car.
- Two cars on the rails never give way to the bot, and a car it boards is
  the nearest on the rails, not a parked one.
- `ram::ram_cars` is the most expensive system on the main thread over the
  whole errand, about 4 ms a traced frame even without its knocks, and
  nobody has looked at why yet.
