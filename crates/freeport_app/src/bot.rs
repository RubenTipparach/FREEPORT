//! The BOT: a player nobody is playing. It walks the streets of the town
//! the world starts in, walks up to a car and takes it, and drives the
//! road network to the next town, and it says what every frame of that
//! cost and where it went wrong.
//!
//! It is a PLAYER and not a camera on a rail, which is the whole point of
//! it: it walks with the walker's own `Input` and is stopped by the same
//! walls, it takes a car through `drive::board` at a player's own reach,
//! so it has to get to one first, and it drives with the scripted drive
//! that follows the route the map plans. What it measures is what a
//! frame costs doing what this game is about, which is getting from one
//! town to another, and where it gets stuck is where a player would.
//!
//! `--bot` runs it in the window and hands the controls back when it is
//! done. `--bot-report out.json` runs it MEASURED and quits, which is what
//! `tools/bench.py` drives: the flight benchmark's own rules (no frame
//! cap, the window drawing unfocused, the keys taken away) and the whole
//! world's clock stepped a sixtieth a frame, so the cars it walks to are
//! where they were on the last run and two runs are one errand.

use crate::drive::{Autopilot, Goal, Thefts};
use crate::stream::Streamer;
use crate::traffic::Crowds;
use crate::walk::OnFoot;
use crate::world::{Fabric, Surface, World};
use crate::{Args, Eye, Status};
use bevy::ecs::system::SystemParam;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use freeport_core::driver::REACH;
use freeport_core::town::{self, lot_frame, Frame};
use freeport_core::traffic::{Streets, HALF_STREET};
use freeport_core::walker::{Input, Walker};
use std::time::Instant;

mod report;

/// How many simulated seconds the bot is given by default, walking and
/// driving together: half an hour, which is a long trip to the nearest
/// town at the car's own top speed and never a short one.
pub(crate) const BOT_SECONDS: f64 = 1800.0;
/// How far it walks the streets before it goes for a car, metres, by
/// default: a few blocks, which is a walk through a town and not a stroll
/// to the kerb.
pub(crate) const BOT_WALK: f64 = 200.0;
/// One sixtieth: what the bot steps the walker, the car and, measured, the
/// world's clock by each frame, the walker's own `--walk` rule.
const STEP: f64 = 1.0 / 60.0;
/// Frames the world is given before the bot sets off, on top of the ground
/// and the towns having settled: the flight benchmark's own warm up, so a
/// first frame's shaders compiling is not a walk's cost.
const WARMUP: u64 = 120;
/// Frames past which it sets off whatever the streamer says, which is
/// `shot.rs`'s own ten times over: a world that never settles is a finding,
/// and a bot that never starts says nothing about it.
const SETTLE_LIMIT: u64 = WARMUP * 25;
/// How near a crossing counts as AT it, metres: inside its square.
const AT_NODE: f64 = 2.5;
/// How fast the bot turns, radians a second: a quick mouse and never a
/// snap, so the camera it is watched through does not jump.
const TURN_RATE: f64 = 4.0;
/// How far off its heading a place can be and the bot still run at it,
/// radians. Past that it slows to turn, or it runs wide round a corner.
const RUN_CONE: f64 = 0.8;
/// How far a car may be and still be walked to, metres: a few blocks.
const HAIL: f64 = 300.0;
/// How long it may go after cars before it gives up, seconds.
const HAIL_LIMIT: f64 = 180.0;
/// How often the way to a moving car is planned again, frames.
const REPLAN: u64 = 30;
/// How near the middle of the town it drives to counts as THERE, metres at
/// the least: a village's radius is smaller than its own streets.
const ARRIVE_MIN: f64 = 60.0;
/// How slow a car pulling up counts as stopped, metres a second.
const STOPPED: f64 = 0.3;
/// How long a body may make no ground before that counts as STUCK,
/// seconds, and how little ground is none, metres a second.
const STUCK: f64 = 3.0;
const CRAWL: f64 = 0.5;
/// How many times the walk may get stuck before the bot stops walking and
/// goes for a car: a street it cannot get down is a finding, not a reason
/// to stand there.
const WALK_STUCK: u32 = 3;

/// Where the bot is in its errand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for the ground, the towns and the crowds.
    Settle,
    /// Walking the town's streets toward its middle.
    Walk,
    /// Walking to the nearest car and taking it.
    Hail,
    /// At the wheel, on the route to the next town.
    Drive,
    /// Finished, one way or another.
    Done,
}

/// One frame as the report and its CSV see it. `wall_ms` is the whole
/// frame, start to next start; `update_ms` the main thread's own part of
/// it, `First` to `Last`.
#[derive(Clone, Debug, Default)]
struct Row {
    label: &'static str,
    wall_ms: f64,
    update_ms: f64,
    speed: f64,
    to_goal: Option<f64>,
    loaded: usize,
    pending: usize,
    /// The finest level of ground DRAWN under the bot (`Streamer::drawn_level`).
    drawn: Option<u8>,
}

/// A walk through one town's streets: which town, its frame, its graph,
/// and the crossings still to reach, in the town's own metres.
struct Streetwise {
    frame: Frame,
    streets: Streets,
    path: Vec<DVec2>,
    next: usize,
}

/// What the bot's legs do this frame, which `walk` takes in place of the
/// keys. Nothing is the keys.
#[derive(Resource, Default)]
pub(crate) struct Legs(pub Option<Input>);

/// The errand, and everything measured on it.
#[derive(Resource)]
pub(crate) struct Bot {
    phase: Phase,
    /// Ask `board` for the car within reach this frame, or out of the one
    /// it is in.
    board: bool,
    frames: u64,
    /// Simulated seconds since the walk began, and when this phase began.
    sim: f64,
    since: f64,
    marks: Vec<(&'static str, f64)>,
    town: Option<Streetwise>,
    was: Option<DVec3>,
    walked: f64,
    driven: f64,
    still: f64,
    stuck: u32,
    walk_stuck: u32,
    /// The town it drives to, and how far off that was when it set off.
    goal: Option<(usize, f64)>,
    outcome: Option<&'static str>,
    rows: Vec<Row>,
    open: Option<Row>,
    clock: Option<Instant>,
    born: Instant,
    walking_from: Option<Instant>,
    settled_s: Option<f64>,
    written: bool,
    /// What it was doing on the last frame, as the CSV says it.
    label: &'static str,
    /// When the last picture was taken, simulated seconds.
    shot_at: Option<f64>,
}

impl Bot {
    fn new() -> Self {
        Self {
            phase: Phase::Settle,
            board: false,
            frames: 0,
            sim: 0.0,
            since: 0.0,
            marks: Vec::new(),
            town: None,
            was: None,
            walked: 0.0,
            driven: 0.0,
            still: 0.0,
            stuck: 0,
            walk_stuck: 0,
            goal: None,
            outcome: None,
            rows: Vec::new(),
            open: None,
            clock: None,
            born: Instant::now(),
            walking_from: None,
            settled_s: None,
            written: false,
            label: "settle",
            shot_at: None,
        }
    }

    /// Whether `board` should act as though E were pressed, which it asks
    /// once and so clears.
    pub(crate) fn take_board(&mut self) -> bool {
        std::mem::take(&mut self.board)
    }

    fn enter(&mut self, phase: Phase) {
        info!("the bot {} after {:.1} s", doing(phase), self.sim);
        self.phase = phase;
        self.since = self.sim;
        self.still = 0.0;
        self.marks.push((name(phase), self.sim));
    }

    fn end(&mut self, why: &'static str) {
        if self.phase != Phase::Done {
            self.outcome.get_or_insert(why);
            self.enter(Phase::Done);
        }
    }

    /// Count a stretch of making no ground as one STUCK, once per stretch.
    fn watch_ground(&mut self, made: f64, dt: f64) {
        if made >= CRAWL * dt {
            self.still = 0.0;
            return;
        }
        self.still += dt;
        if self.still > STUCK {
            self.still = 0.0;
            self.stuck += 1;
            if self.phase == Phase::Walk {
                self.walk_stuck += 1;
            }
        }
    }
}

fn name(phase: Phase) -> &'static str {
    match phase {
        Phase::Settle => "settle",
        Phase::Walk => "walk",
        Phase::Hail => "hail",
        Phase::Drive => "drive",
        Phase::Done => "done",
    }
}

fn doing(phase: Phase) -> &'static str {
    match phase {
        Phase::Settle => "waits for the world",
        Phase::Walk => "walks the streets",
        Phase::Hail => "goes for a car",
        Phase::Drive => "is at the wheel",
        Phase::Done => "is done",
    }
}

pub(crate) struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        let on = resource_exists::<Bot>;
        app.init_resource::<Legs>()
            .init_resource::<Autopilot>()
            .add_systems(Startup, start)
            .add_systems(First, clock_in.run_if(on))
            .add_systems(
                Update,
                (
                    steer.before(crate::walk::walk).before(crate::drive::board),
                    (watch, photograph, finish)
                        .chain()
                        .after(crate::drive::drive_car),
                )
                    .run_if(on),
            )
            .add_systems(Last, clock_out.run_if(on));
    }
}

/// `--drive`'s own autopilot, and the bot if one was asked for.
fn start(
    mut commands: Commands,
    mut args: ResMut<Args>,
    mut pilot: ResMut<Autopilot>,
    mut clock: ResMut<Time<Virtual>>,
) {
    pilot.frames = args.drive;
    if !args.bot {
        return;
    }
    // The bot steps a sixtieth a frame. Measured, the world's clock does
    // the same so the rails and the sun agree with it, and it is HELD
    // while the world comes up: the settle is a different number of
    // frames every run, and a clock that ran through it would put every
    // car the bot walks to somewhere else each time. In the window it is
    // held to sixty so a player watching sees it at its own pace.
    if args.measuring() {
        commands.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(STEP),
        ));
        clock.pause();
    } else if args.fps > 60.0 || args.fps <= 0.0 {
        args.fps = 60.0;
    }
    commands.insert_resource(Bot::new());
}

fn clock_in(mut bot: ResMut<Bot>) {
    let now = Instant::now();
    if let (Some(mut row), Some(was)) = (bot.open.take(), bot.clock) {
        row.wall_ms = (now - was).as_secs_f64() * 1e3;
        bot.rows.push(row);
    }
    bot.clock = Some(now);
    bot.open = Some(Row::default());
}

fn clock_out(mut bot: ResMut<Bot>) {
    let Some(started) = bot.clock else { return };
    if let Some(row) = bot.open.as_mut() {
        row.update_ms = started.elapsed().as_secs_f64() * 1e3;
    }
}

/// What the bot looks at to decide where its legs go.
#[derive(SystemParam)]
pub(crate) struct Sight<'w> {
    here: Surface<'w>,
    streamer: Option<Res<'w, Streamer>>,
    crowds: Option<Res<'w, Crowds>>,
    walker: Option<Res<'w, OnFoot>>,
    time: Res<'w, Time>,
    args: Res<'w, Args>,
}

/// Where the legs go this frame, and whether to take the car in reach.
fn steer(
    mut bot: ResMut<Bot>,
    sight: Sight,
    mut legs: ResMut<Legs>,
    mut pilot: ResMut<Autopilot>,
    mut clock: ResMut<Time<Virtual>>,
) {
    legs.0 = None;
    bot.frames += 1;
    match bot.phase {
        Phase::Settle => {
            settle(&mut bot, &sight);
            // The world's clock starts when the walk does (`start`).
            if bot.phase != Phase::Settle {
                clock.unpause();
            }
        }
        Phase::Walk => legs.0 = walk_on(&mut bot, &sight),
        Phase::Hail => legs.0 = hail(&mut bot, &sight, &mut pilot),
        Phase::Drive | Phase::Done => {}
    }
}

/// Set off once the ground, the towns and the crowds are up: a player
/// waits for them behind a loading screen this game does not have.
fn settle(bot: &mut Bot, sight: &Sight) {
    let ground = sight.streamer.as_ref().is_none_or(|s| s.idle());
    let towns = sight.here.fabric.settled();
    let ready = ground && towns && sight.crowds.is_some();
    if bot.frames < WARMUP || !(ready || bot.frames > SETTLE_LIMIT) {
        return;
    }
    if sight.walker.is_none() {
        bot.end("the bot starts on foot, and this run started in the air or in a car");
        return;
    }
    bot.settled_s = Some(bot.born.elapsed().as_secs_f64());
    if !ready {
        warn!("the bot sets off before the world settled");
    }
    bot.walking_from = Some(Instant::now());
    bot.enter(Phase::Walk);
}

/// Along the town's streets toward its middle, crossing by crossing.
fn walk_on(bot: &mut Bot, sight: &Sight) -> Option<Input> {
    let w = &sight.walker.as_ref()?.0;
    let here = w.dir * w.foot;
    if bot.town.is_none() {
        bot.town = tour(sight.here.world(), here, sight.args.bot_walk);
    }
    let far = bot.walked >= sight.args.bot_walk || bot.walk_stuck >= WALK_STUCK;
    let next = bot.town.as_mut().and_then(|t| t.ahead(here));
    match next {
        Some(target) if !far => Some(legs_toward(w, target)),
        _ => {
            bot.enter(Phase::Hail);
            None
        }
    }
}

/// To the nearest car, along the streets while it is more than a block
/// off and straight at it once it is not, and into it once it is within a
/// player's reach.
fn hail(bot: &mut Bot, sight: &Sight, pilot: &mut Autopilot) -> Option<Input> {
    if bot.sim - bot.since > HAIL_LIMIT {
        bot.end("no car came within reach");
        return None;
    }
    let w = &sight.walker.as_ref()?.0;
    let world = sight.here.world();
    let here = w.dir * w.foot;
    if bot.town.is_none() {
        bot.town = tour(world, here, 0.0);
    }
    let crowds = sight.crowds.as_ref()?;
    let standing = sight.here.fabric.standing();
    let now = sight.time.elapsed_secs_f64();
    // None within reach is a wait on the kerb for one to come round,
    // which the rails promise.
    let car = crowds
        .cars_near(world.planet.radius, here, HAIL, now, &standing)
        .into_iter()
        .map(|c| c.2)
        .min_by(|a, b| a.distance(here).total_cmp(&b.distance(here)))?;
    let away = car.distance(here);
    if away < REACH * 0.9 {
        // Handed over BEFORE `board`, which runs this frame and whose
        // first frame at the wheel reads it.
        let left = (sight.args.bot_seconds - bot.sim).max(0.0);
        pilot.realtime = !sight.args.bot_fast;
        pilot.frames = if pilot.realtime { left / STEP } else { left } as u32;
        bot.board = true;
        return None;
    }
    let replan = bot.frames.is_multiple_of(REPLAN);
    let street = (away > town::PITCH)
        .then(|| bot.town.as_mut()?.toward(here, car, replan))
        .flatten();
    Some(legs_toward(w, street.unwrap_or(car)))
}

impl Streetwise {
    /// The next crossing still to reach, passing any the body is at.
    fn ahead(&mut self, here: DVec3) -> Option<DVec3> {
        let p = flat(&self.frame, here);
        while self.next < self.path.len() && (self.path[self.next] - p).length() < AT_NODE {
            self.next += 1;
        }
        let n = self.path.get(self.next)?;
        Some(self.frame.world(DVec3::new(n.x, n.y, 0.0)))
    }

    /// The next crossing toward a place that moves, planned again when
    /// asked to and whenever the last plan has been walked. An EMPTY plan
    /// waits for the next time it is asked: the paving never joined the
    /// two, and a search every frame would be a cost the bot measures.
    fn toward(&mut self, here: DVec3, goal: DVec3, replan: bool) -> Option<DVec3> {
        if replan || (!self.path.is_empty() && self.next >= self.path.len()) {
            let p = flat(&self.frame, here);
            self.path = self.streets.route(p, flat(&self.frame, goal));
            self.next = behind(&self.path, p);
        }
        self.ahead(here)
    }
}

/// The walk: from where the body stands along the town's own streets to
/// the crossing nearest its middle, or, from the middle already, to one
/// `far` metres east of it.
fn tour(world: &World, at: DVec3, far: f64) -> Option<Streetwise> {
    let radius = world.planet.radius;
    let dir = at.normalize();
    let town = world.towns.iter().min_by(|a, b| {
        a.dir
            .angle_between(dir)
            .total_cmp(&b.dir.angle_between(dir))
    })?;
    let frame = lot_frame(radius, town, 0.0, 0.0);
    let streets = Streets::of(town);
    let from = flat(&frame, at);
    let to = if from.length() > 30.0 {
        DVec2::ZERO
    } else {
        DVec2::new(far, 0.0)
    };
    let path = streets.route(from, to);
    let next = behind(&path, from);
    Some(Streetwise {
        frame,
        streets,
        path,
        next,
    })
}

/// Which crossing of a fresh route to head for first: the second when the
/// body is already on the street between the first two, because the
/// first is then behind it and walking back to it is a zigzag.
fn behind(path: &[DVec2], p: DVec2) -> usize {
    let [a, b, ..] = path else { return 0 };
    let ab = *b - *a;
    let t = (p - *a).dot(ab) / ab.length_squared().max(1e-9);
    let off = ((p - *a) - ab * t).length();
    usize::from((0.0..=1.0).contains(&t) && off < HALF_STREET)
}

fn flat(frame: &Frame, p: DVec3) -> DVec2 {
    let l = frame.local(p);
    DVec2::new(l.x, l.y)
}

/// The walker's own input toward a place: run at it, turning no faster
/// than `TURN_RATE`, and slow down to turn so a corner is walked round
/// rather than overshot. A positive turn is to the LEFT (`Walker::turn`).
fn legs_toward(w: &Walker, target: DVec3) -> Input {
    let aim = target.normalize();
    let to = (aim - w.dir * aim.dot(w.dir)).normalize_or(w.fwd);
    let off = w.fwd.cross(to).dot(w.dir).atan2(w.fwd.dot(to));
    Input {
        forward: if off.abs() < RUN_CONE { 1.0 } else { 0.2 },
        run: true,
        turn: off.clamp(-TURN_RATE * STEP, TURN_RATE * STEP),
        ..Default::default()
    }
}

/// What the bot looks at after the frame has moved it.
#[derive(SystemParam)]
pub(crate) struct Watched<'w> {
    here: Surface<'w>,
    thefts: Res<'w, Thefts>,
    walker: Option<Res<'w, OnFoot>>,
    goal: Res<'w, Goal>,
    streamer: Option<Res<'w, Streamer>>,
    args: Res<'w, Args>,
    pilot: ResMut<'w, Autopilot>,
}

/// How far the frame took it, whether it has got there, and the row the
/// frame is written down as.
fn watch(mut bot: ResMut<Bot>, mut seen: Watched, mut status: ResMut<Status>) {
    let world = seen.here.world();
    let radius = world.planet.radius;
    let (at, speed, driving) = match (seen.thefts.driving(), &seen.walker) {
        (Some(t), _) => (t.car.dir * t.car.foot, t.car.speed.abs(), true),
        (None, Some(w)) => (w.0.dir * w.0.foot, w.0.vel[0].hypot(w.0.vel[1]), false),
        (None, None) => return,
    };
    if bot.phase == Phase::Hail && driving {
        bot.enter(Phase::Drive);
    }
    let dt = if driving && seen.args.bot_fast {
        1.0
    } else {
        STEP
    };
    let made = bot.was.map_or(0.0, |w| w.angle_between(at) * radius);
    bot.was = Some(at);
    match bot.phase {
        Phase::Walk | Phase::Hail => {
            bot.sim += dt;
            bot.walked += made;
        }
        Phase::Drive => {
            bot.sim += dt;
            bot.driven += made;
        }
        Phase::Settle | Phase::Done => {}
    }
    if matches!(bot.phase, Phase::Walk | Phase::Drive) {
        bot.watch_ground(made, dt);
    }
    let (to_goal, park) = arrive(&mut bot, &seen, (at, speed), driving);
    if bot.sim > seen.args.bot_seconds {
        bot.end("ran out of time");
    }
    let label = match bot.phase {
        Phase::Drive if in_town(world, at) => "streets",
        Phase::Drive => "highway",
        phase => name(phase),
    };
    bot.label = label;
    if park {
        seen.pilot.park = true;
    }
    let stats = seen
        .streamer
        .as_ref()
        .map(|s| (s.stats.loaded, s.stats.pending, s.drawn_level(at)));
    if let Some(row) = bot.open.as_mut() {
        *row = Row {
            label,
            speed,
            to_goal,
            loaded: stats.map_or(0, |s| s.0),
            pending: stats.map_or(0, |s| s.1),
            drawn: stats.and_then(|s| s.2),
            ..row.clone()
        };
    }
    status.walker = format!(
        "bot: {label}, {:.0} m walked, {:.0} m driven{}",
        bot.walked,
        bot.driven,
        to_goal.map_or(String::new(), |d| format!(", {:.2} km to go", d / 1000.0)),
    );
}

/// How far the town it drives to is, and whether it is there: in which
/// case it gets out, and the errand is done.
///
/// THERE is inside the town's own outline, which is where its streets are
/// and where the drive's own labels call it a town: the nominal radius was
/// asked for first, and the car came within 267 m of a village's middle,
/// inside the village, and drove on past it at 160 km/h. There, it pulls
/// up, and once it has stopped the bot gets out.
fn arrive(
    bot: &mut Bot,
    seen: &Watched,
    (at, speed): (DVec3, f64),
    driving: bool,
) -> (Option<f64>, bool) {
    let world = seen.here.world();
    let Some(goal) = seen.goal.0 else {
        return (None, false);
    };
    let away = goal.angle_between(at) * world.planet.radius;
    if bot.goal.is_none() {
        let Some(k) = world.towns.iter().position(|t| t.dir == goal) else {
            return (Some(away), false);
        };
        info!("the bot drives for town {k}, {:.2} km off", away / 1000.0);
        bot.goal = Some((k, away));
    }
    let Some((k, _)) = bot.goal else {
        return (Some(away), false);
    };
    let there = (world.towns[k].radius * town::OUTLINE).max(ARRIVE_MIN);
    let park = driving && away < there && bot.phase == Phase::Drive;
    if park && speed < STOPPED {
        bot.board = true;
        bot.end("arrived");
    }
    (Some(away), park)
}

/// Whether a place is on ground a town has levelled, which is where a
/// drive is on its streets rather than on the highway.
///
/// The town's own edge along the bearing (`Site::level_r`) and not the
/// disc it can reach at its widest: a town is squeezed across its shore
/// to under half of that, so the disc counted two kilometres of highway
/// out of a city as streets, and the drive out of the port at 160 km/h
/// on ground too coarse for the road was filed under the town.
fn in_town(world: &World, at: DVec3) -> bool {
    let dir = at.normalize();
    let radius = world.planet.radius;
    world.towns.iter().any(|t| {
        let away = t.dir.angle_between(dir) * radius;
        away < t.radius * town::OUTLINE && away < town::site_of(t).level_r(dir)
    })
}

/// A picture every `--bot-shot-every` simulated seconds of the errand,
/// named by when it was and what the bot was doing, so a run nobody
/// watched can be looked at: the drive out of a town is a thing a
/// picture shows and a frame time does not.
fn photograph(mut commands: Commands, mut bot: ResMut<Bot>, args: Res<Args>) {
    let Some(dir) = &args.bot_shots else { return };
    let busy = matches!(bot.phase, Phase::Walk | Phase::Hail | Phase::Drive);
    let due = bot
        .shot_at
        .is_none_or(|t| bot.sim - t >= args.bot_shot_every);
    if !busy || !due {
        return;
    }
    bot.shot_at = Some(bot.sim);
    let path = std::path::Path::new(dir).join(format!("{:07.1}-{}.png", bot.sim, bot.label));
    if let Err(e) = std::fs::create_dir_all(dir) {
        warn!("no pictures for the bot at {dir}: {e}");
        return;
    }
    commands
        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
        .observe(bevy::render::view::screenshot::save_to_disk(path));
}

/// What the report says the run was, besides the bot's own numbers.
#[derive(SystemParam)]
pub(crate) struct Context<'w, 's> {
    args: Res<'w, Args>,
    streamer: Option<Res<'w, Streamer>>,
    fabric: Res<'w, Fabric>,
    tuning: Res<'w, crate::tuning::Tuning>,
    adapter: Res<'w, bevy::render::renderer::RenderAdapterInfo>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    eye: Res<'w, Eye>,
}

/// Once it is done, say how it went, and measured, write it down and quit.
fn finish(mut bot: ResMut<Bot>, context: Context, mut exit: MessageWriter<AppExit>) {
    if bot.phase != Phase::Done || bot.written {
        return;
    }
    bot.written = true;
    let report = bot.report(&context);
    info!(
        "the bot {}: walked {:.0} m, drove {:.0} m in {:.0} simulated s, stuck {} times; frames {}",
        bot.outcome.unwrap_or("stopped"),
        bot.walked,
        bot.driven,
        bot.sim,
        bot.stuck,
        report["frames"]["all"],
    );
    let Some(path) = &context.args.bot_report else {
        return;
    };
    let json = std::path::Path::new(path);
    let written = serde_json::to_vec_pretty(&report)
        .ok()
        .and_then(|bytes| std::fs::write(json, bytes).ok())
        .and_then(|()| std::fs::write(json.with_extension("frames.csv"), bot.csv()).ok());
    match written {
        Some(()) => info!("the bot's report is at {path}"),
        None => error!("could not write the bot's report to {path}"),
    }
    exit.write(AppExit::Success);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_begun_between_its_first_two_crossings_heads_for_the_second() {
        let path = [
            DVec2::new(0.0, 0.0),
            DVec2::new(40.0, 0.0),
            DVec2::new(40.0, 40.0),
        ];
        // Halfway down the first street: the first crossing is behind.
        assert_eq!(behind(&path, DVec2::new(20.0, 1.0)), 1);
        // Short of it, or off to the side of the street: it is ahead.
        assert_eq!(behind(&path, DVec2::new(-5.0, 0.0)), 0);
        assert_eq!(behind(&path, DVec2::new(20.0, HALF_STREET + 1.0)), 0);
        assert_eq!(behind(&path[..1], DVec2::new(20.0, 0.0)), 0);
    }

    #[test]
    fn the_legs_turn_toward_a_place_no_faster_than_the_turn_rate() {
        let w = Walker {
            dir: DVec3::Y,
            fwd: DVec3::NEG_Z,
            pitch: 0.0,
            h: 0.0,
            vy: 0.0,
            vel: [0.0; 2],
            on_ground: true,
            foot: 1000.0,
        };
        // Dead ahead: full speed, no turn.
        let ahead = legs_toward(&w, DVec3::new(0.0, 1000.0, -10.0));
        assert_eq!(ahead.forward, 1.0);
        assert!(ahead.turn.abs() < 1e-9);
        // Off to the LEFT, which with up as Y and forward as -Z is -X: a
        // positive turn, capped, and slowed for it.
        let left = legs_toward(&w, DVec3::new(-10.0, 1000.0, 0.0));
        assert!((left.turn - TURN_RATE * STEP).abs() < 1e-12);
        assert!(left.forward < 1.0);
        let mut turned = w.clone();
        turned.turn(left.turn);
        assert!(turned.fwd.x < 0.0, "a positive turn is a left turn");
        // And the right is the other way.
        let right = legs_toward(&w, DVec3::new(10.0, 1000.0, 0.0));
        assert!((right.turn + TURN_RATE * STEP).abs() < 1e-12);
    }
}
