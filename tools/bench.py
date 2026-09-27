#!/usr/bin/env python3
"""Drive the game with its own BOT and say what a frame costs doing what
the game is about, and whether a change moved it.

The bot (`crates/freeport_app/src/bot.rs`) is a player nobody is playing:
it walks the streets of the port to a car, takes it, and drives the road
network to the next town. This runs it in `--bot-report` mode, one errand
at a time, keeps what every run wrote, and turns the rounds into medians
with their spread beside them, because a number with no spread is a number
nobody can tell a change from noise with. Standard library only, like
`pngdiff.py`, because the containers this project is built in do not all
carry numpy.

    python tools/bench.py list                             # the errands
    python tools/bench.py run                              # the default errands, three rounds
    python tools/bench.py run --scenarios town --rounds 1  # the quick one
    python tools/bench.py run --label after --baseline target/bench/<dir>
    python tools/bench.py ab --base old/freeport_app.exe --head target/release/freeport_app.exe
    python tools/bench.py compare target/bench/<before> target/bench/<after>
    python tools/bench.py spikes target/bench/<dir>/runs/<run>.frames.csv
    python tools/bench.py show target/bench/<dir>

Everything lands in `target/bench/<stamp>-<label>/`: the machine and the
binary it measured (`meta.json`), every run's own report, per frame CSV and
log (`runs/`), the medians (`summary.json`) and a table fit for a commit
message (`report.md`).

What it refuses and what it only says. A second game, or a screen recorder,
is refused: two games share one GPU and both their frame times lie, and a
recording on this laptop is somebody else's measurement. A compiler or a
linker running is refused unless `--force`, because
`docs/flight-performance.md` discarded a whole round of numbers taken beside
a release build. A laptop on BATTERY, a high CPU load before a run, a game
that came up on the integrated GPU of a machine that has a discrete one, an
errand that did not end where its other rounds ended, and one that did not
arrive are all said in the report, because each is a reason a number is not
the number it looks like, and none of them is this tool's to fix.

An A/B interleaves the two binaries ABBA round by round, so a laptop warming
up over twenty minutes charges both sides alike rather than whichever ran
second.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE_NAME = "freeport_app.exe" if os.name == "nt" else "freeport_app"
DEFAULT_EXE = ROOT / "target" / "release" / EXE_NAME
OUT_ROOT = ROOT / "target" / "bench"
# A string only a binary with the bot in it carries: a key of its report.
BOT_MARK = b"wall_seconds_by_phase"


@dataclass(frozen=True)
class Scenario:
    """One errand: what it is, and the flags that run it."""

    about: str
    args: tuple[str, ...]
    default: bool = True
    # Seconds of WALL time a round may take before it is killed.
    timeout: float = 900.0


# Every errand is the bot's: it waits for the ground, the towns and the
# crowds to settle and 120 frames on top, walks `--bot-walk` metres of the
# port's streets, walks to the nearest car and takes it, and drives the
# route the map plans to the nearest other town, stepping the walker, the
# car and the world's clock a fixed sixtieth a frame so two runs are one
# errand. What differs between errands is how long it is given.
SCENARIOS: dict[str, Scenario] = {
    "town": Scenario(
        "walk 200 m of the port, take a car, and drive out through its streets: "
        "two simulated minutes, the quick one",
        ("--bot-seconds", "120"),
        timeout=600.0,
    ),
    "trip": Scenario(
        "the whole errand: walk, take a car, and drive the road to the next town until it arrives "
        "(half an hour of simulated time at most)",
        ("--bot-seconds", "1800"),
        timeout=3600.0,
    ),
    "trip-fast": Scenario(
        "the same trip driven a second a frame, for a software rasteriser that wants to know the "
        "bot ARRIVES and not what its frames cost: the drive's frame times mean nothing here",
        ("--bot-seconds", "1800", "--bot-fast"),
        default=False,
        timeout=3600.0,
    ),
}

# What is read out of a report, where, and which way is better: -1 is lower
# is better, 0 is a CHECK (the same errand should walk and drive the same
# distance) rather than a score. A path the report does not carry is
# skipped, so an older binary with fewer fields still compares on the ones
# it has. The labels are the bot's own: walking, driving a town's streets,
# and the highway between two towns.
METRICS: list[tuple[str, str, int]] = [
    ("frames.all.p50_ms", "frame p50 ms", -1),
    ("frames.all.p95_ms", "frame p95 ms", -1),
    ("frames.all.p99_ms", "frame p99 ms", -1),
    ("frames.all.max_ms", "frame worst ms", -1),
    ("frames.all.over_16_67_ms", "frames over 16.7 ms", -1),
    ("frames.all.over_33_33_ms", "frames over 33.3 ms", -1),
    ("frames.walk.p99_ms", "walking p99 ms", -1),
    ("frames.streets.p99_ms", "town driving p99 ms", -1),
    ("frames.highway.p99_ms", "highway p99 ms", -1),
    ("update_cpu.all.p50_ms", "update p50 ms", -1),
    ("update_cpu.all.p99_ms", "update p99 ms", -1),
    ("settle_seconds", "settle s", -1),
    ("wall_seconds", "errand wall s", -1),
    ("terrain.mean_chunk_ms", "mean chunk ms", -1),
    ("terrain.remaining", "chunks still wanted at the end", -1),
    ("stuck", "times stuck", -1),
    ("sim_seconds", "simulated s", 0),
    ("walked_m", "walked m", 0),
    ("driven_m", "driven m", 0),
]

# Processes whose presence makes a frame time a lie. A game or a recorder
# is always refused; a compiler or a linker is refused unless forced.
GAMES = {"freeport_app", "pbd-app", "obs64", "obs32", "obs"}
BUILDERS = {"cargo", "rustc", "link", "rust-lld", "lld-link", "ld", "ld.lld", "mold", "cc1", "cc1plus"}

# ------------------------------------------------------------------ machine --


def powershell_json(script: str) -> object | None:
    """Run a PowerShell snippet that ends in ConvertTo-Json and parse it."""
    try:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            capture_output=True, text=True, timeout=60,
        ).stdout
        return json.loads(out) if out.strip() else None
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        return None


def machine() -> dict:
    """What the numbers were measured on: CPU, GPUs, memory and power."""
    info: dict = {
        "os": platform.platform(),
        "python": platform.python_version(),
        "logical_cpus": os.cpu_count(),
    }
    if os.name == "nt":
        got = powershell_json(
            "$p = Get-CimInstance Win32_Processor;"
            "$b = @(Get-CimInstance Win32_Battery);"
            "$s = powercfg /getactivescheme;"
            "[ordered]@{cpu = @($p.Name)[0];"
            " gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name + ' ' + $_.DriverVersion });"
            " memory_gb = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1);"
            " battery_status = if ($b.Count) { $b[0].BatteryStatus } else { $null };"
            " power_scheme = \"$s\".Trim()} | ConvertTo-Json -Compress"
        )
        if isinstance(got, dict):
            info.update(got)
            # powercfg says "Power Scheme GUID: <guid>  (Name)"; the name is
            # what a person reads, and the GUID is kept beside it.
            scheme = str(got.get("power_scheme") or "")
            if "(" in scheme:
                info["power_scheme"] = scheme[scheme.rindex("(") + 1:].rstrip(")")
                info["power_scheme_guid"] = scheme.split(":", 1)[-1].split("(")[0].strip()
            # Win32_Battery: 1 discharging, 4 low, 5 critical. Every other
            # status is on the mains; no battery at all is a desktop.
            status = got.get("battery_status")
            info["on_battery"] = None if status is None else status in (1, 4, 5)
    else:
        info.update(linux_machine())
    return info


def linux_machine() -> dict:
    info: dict = {}
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                info["cpu"] = line.split(":", 1)[1].strip()
                break
        info["memory_gb"] = round(os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES") / 2**30, 1)
    except (OSError, ValueError):
        pass
    mains = [p for p in Path("/sys/class/power_supply").glob("*") if (p / "online").exists()]
    if mains:
        info["on_battery"] = not any((p / "online").read_text().strip() == "1" for p in mains)
    return info


def process_names() -> list[str]:
    """The names of every running process, lower case."""
    try:
        if os.name == "nt":
            out = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True, timeout=30).stdout
            return [row.split('","')[0].strip('"').lower() for row in out.splitlines() if row.startswith('"')]
        out = subprocess.run(["ps", "-eo", "comm="], capture_output=True, text=True, timeout=30).stdout
        return [line.strip().lower() for line in out.splitlines() if line.strip()]
    except (OSError, subprocess.TimeoutExpired):
        return []


def in_the_way() -> tuple[list[str], list[str]]:
    """Games and builders running now, each by name."""
    names = process_names()
    strip = lambda n: n[:-4] if n.endswith(".exe") else n  # noqa: E731
    games = sorted({n for n in names if strip(n) in GAMES})
    builders = sorted({n for n in names if strip(n) in BUILDERS})
    return games, builders


def cpu_load() -> float | None:
    """The machine's CPU load right now, percent, averaged over a second or so."""
    if os.name == "nt":
        got = powershell_json(
            "(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average"
            " | ConvertTo-Json"
        )
        return float(got) if isinstance(got, (int, float)) else None
    try:
        one = float(Path("/proc/loadavg").read_text().split()[0])
        return round(100.0 * one / (os.cpu_count() or 1), 1)
    except (OSError, ValueError, IndexError):
        return None


def git_state() -> dict:
    def git(*args: str) -> str:
        try:
            return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, timeout=30).stdout.strip()
        except (OSError, subprocess.TimeoutExpired):
            return ""

    return {
        "commit": git("rev-parse", "HEAD"),
        "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
        "dirty": bool(git("status", "--porcelain", "--untracked-files=no")),
    }


def binary(exe: Path) -> dict:
    """What was measured: the file, its hash, and whether it is older than the source."""
    data = exe.read_bytes()
    built = exe.stat().st_mtime
    newest = max(
        (p.stat().st_mtime for pattern in ("crates/**/*.rs", "crates/**/*.wgsl", "Cargo.lock")
         for p in ROOT.glob(pattern)),
        default=0.0,
    )
    return {
        "path": str(exe),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
        "built_utc": datetime.fromtimestamp(built, timezone.utc).isoformat(),
        "older_than_source": newest > built,
        # A binary from before the bot takes `--bot-report` for an unknown
        # flag and sits in the game until the timeout. The flag itself is
        # compiled into integer compares and is not in the file, but the
        # report's own `wall_seconds_by_phase` key is a string literal, so
        # its absence says so without running anything (`bot.rs` notes it).
        "has_bot": BOT_MARK in data,
    }


# ------------------------------------------------------------------- a run --


def display_wrapper() -> list[str]:
    """A virtual display on a Linux box with none: Xvfb and lavapipe, the rig
    `run.sh --shot` uses. Its frame times are for A/B only, never absolute."""
    if sys.platform.startswith("linux") and not os.environ.get("DISPLAY") and shutil.which("xvfb-run"):
        return ["xvfb-run", "-a", "-s", "-screen 0 1280x720x24"]
    return []


def run_env() -> dict:
    env = dict(os.environ)
    icd = "/usr/share/vulkan/icd.d/lvp_icd.json"
    if display_wrapper() and Path(icd).exists():
        env.setdefault("VK_ICD_FILENAMES", icd)
    return env


def run_one(exe: Path, name: str, report: Path, log: Path, timeout: float | None, extra: list[str]) -> dict:
    """Run one errand once and say how it went; the report is the game's own."""
    scenario = SCENARIOS[name]
    timeout = timeout or scenario.timeout
    cmd = [*display_wrapper(), str(exe), "--bot-report", str(report), *scenario.args, *extra]
    record: dict = {"scenario": name, "cmd": cmd[len(display_wrapper()):], "load_before": cpu_load()}
    report.unlink(missing_ok=True)
    started = time.perf_counter()
    with open(log, "wb") as out:
        child = subprocess.Popen(cmd, cwd=ROOT, stdout=out, stderr=subprocess.STDOUT, env=run_env())
        try:
            record["exit_code"] = child.wait(timeout=timeout)
            record["timed_out"] = False
        except subprocess.TimeoutExpired:
            # The child this run started and nothing else.
            child.kill()
            child.wait()
            record["exit_code"], record["timed_out"] = None, True
    record["wall_s"] = round(time.perf_counter() - started, 2)
    record["report"] = str(report) if report.exists() else None
    return record


def load(path: str | None) -> dict | None:
    if not path:
        return None
    try:
        return json.loads(Path(path).read_text())
    except (OSError, json.JSONDecodeError):
        return None


def dig(report: dict, path: str) -> float | None:
    """A number at a dotted path, or None if the report does not carry it."""
    value: object = report
    for key in path.split("."):
        if not isinstance(value, dict) or key not in value:
            return None
        value = value[key]
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


# ----------------------------------------------------------------- summary --


def summarise(runs: list[dict], gpus: list[str]) -> dict:
    """Medians and spreads per route and side, with what makes them suspect.
    `gpus` is what the machine has, for telling integrated from discrete."""
    groups: dict[tuple[str, str], list[dict]] = {}
    for run in runs:
        groups.setdefault((run["scenario"], run.get("side", "head")), []).append(run)
    out: dict = {}
    for (name, side), group in sorted(groups.items()):
        reports = [r for r in (load(run.get("report")) for run in group) if r]
        metrics = {}
        for path, _, _ in METRICS:
            values = [v for v in (dig(r, path) for r in reports) if v is not None]
            if values:
                metrics[path] = {
                    "median": statistics.median(values),
                    "min": min(values),
                    "max": max(values),
                    "values": values,
                }
        out.setdefault(name, {})[side] = {
            "rounds": len(group),
            "reported": len(reports),
            "outcomes": [r.get("outcome") for r in reports],
            "metrics": metrics,
            "warnings": warnings(group, reports, gpus),
        }
    return out


def warnings(group: list[dict], reports: list[dict], gpus: list[str]) -> list[str]:
    """Every reason these rounds are not the numbers they look like."""
    said = []
    failed = [r for r in group if not r.get("report")]
    if failed:
        said.append(f"{len(failed)} of {len(group)} rounds wrote no report (see their logs)")
    loads = [r["load_before"] for r in group if isinstance(r.get("load_before"), (int, float))]
    if loads and max(loads) > 25:
        said.append(f"the CPU was {max(loads):.0f}% busy before a round: something else was running")
    used = {r.get("gpu") for r in reports}
    if len(used) > 1:
        said.append(f"rounds ran on different GPUs: {sorted(map(str, used))}")
    discrete = any(word in " ".join(gpus).lower() for word in ("nvidia", "radeon rx", "arc a"))
    for gpu in used:
        if gpu and "intel" in gpu.lower() and "arc" not in gpu.lower() and discrete:
            said.append(f"ran on {gpu} although a discrete GPU is installed")
    ends = [r.get("end") for r in reports if isinstance(r.get("end"), list)]
    if len(ends) > 1:
        spread = max(sum((a - b) ** 2 for a, b in zip(e, ends[0])) ** 0.5 for e in ends)
        if spread > 1.0:
            said.append(f"the route ended {spread:.1f} m apart across rounds: it is not one route")
    outcomes = [r.get("outcome") for r in reports]
    if any(o != "arrived" for o in outcomes) and group and group[0]["scenario"].startswith("trip"):
        said.append(f"not every trip arrived: {outcomes}")
    if len(set(outcomes)) > 1:
        said.append(f"the rounds ended differently: {outcomes}")
    unfocused = sum(1 for r in reports if r.get("window_focused_at_finish") is False)
    if unfocused:
        said.append(f"{unfocused} rounds finished with the window unfocused")
    remaining = [dig(r, "terrain.remaining") or 0 for r in reports]
    if remaining and max(remaining) > 0:
        said.append(
            f"the streamer still wanted up to {max(remaining):.0f} chunks at the end: "
            "a faster frame here can be a frame that streamed less"
        )
    return said


def fmt(value: float | None) -> str:
    if value is None:
        return "-"
    if abs(value) >= 100 or value == int(value):
        return f"{value:,.0f}"
    return f"{value:.2f}"


def table(summary: dict) -> str:
    """One side's medians, a table per route."""
    lines = []
    for name, sides in summary.items():
        for side, s in sides.items():
            outcomes = ", ".join(sorted(set(map(str, s.get("outcomes", []))))) or "none"
            lines.append(f"### {name} ({side}, {s['reported']} of {s['rounds']} rounds: {outcomes})\n")
            lines.append("| metric | median | min | max |")
            lines.append("| --- | ---: | ---: | ---: |")
            for path, label, _ in METRICS:
                m = s["metrics"].get(path)
                if m:
                    lines.append(f"| {label} | {fmt(m['median'])} | {fmt(m['min'])} | {fmt(m['max'])} |")
            lines += [f"\n- {w}" for w in s["warnings"]]
            lines.append("")
    return "\n".join(lines)


def comparison(base: dict, head: dict, base_side: str = "head", head_side: str = "head") -> str:
    """Base against head per route: the medians, the change, and whether the
    change is bigger than either side's own spread across its rounds."""
    lines = []
    for name in sorted(set(base) & set(head)):
        b, h = base[name].get(base_side), head[name].get(head_side)
        if not b or not h:
            continue
        lines.append(f"### {name} (base {b['reported']} rounds, head {h['reported']} rounds)\n")
        lines.append("| metric | base | head | change | spread | verdict |")
        lines.append("| --- | ---: | ---: | ---: | ---: | --- |")
        for path, label, better in METRICS:
            mb, mh = b["metrics"].get(path), h["metrics"].get(path)
            if not mb or not mh:
                continue
            delta = mh["median"] - mb["median"]
            pct = f"{100 * delta / mb['median']:+.1f}%" if mb["median"] else "-"
            spread = max(mb["max"] - mb["min"], mh["max"] - mh["min"])
            lines.append(
                f"| {label} | {fmt(mb['median'])} | {fmt(mh['median'])} | {pct} | {fmt(spread)} | "
                f"{verdict(delta, spread, better, min(b['reported'], h['reported']))} |"
            )
        lines += [f"\n- base: {w}" for w in b["warnings"]]
        lines += [f"\n- head: {w}" for w in h["warnings"]]
        lines.append("")
    return "\n".join(lines) or "no route was measured on both sides\n"


def verdict(delta: float, spread: float, better: int, rounds: int) -> str:
    if better == 0:
        return "same" if abs(delta) < 1.0 else "DIFFERENT ROUTE"
    if rounds < 2:
        return "one round, no spread"
    if abs(delta) <= spread:
        return "within noise"
    return "better" if delta * better > 0 else "worse"


# -------------------------------------------------------------------- CLI --


def pick(names: str | None) -> list[str]:
    if not names:
        return [n for n, s in SCENARIOS.items() if s.default]
    if names == "all":
        return list(SCENARIOS)
    chosen = [n.strip() for n in names.split(",") if n.strip()]
    unknown = [n for n in chosen if n not in SCENARIOS]
    if unknown:
        raise SystemExit(f"unknown route {unknown}; `bench.py list` names them")
    return chosen


def guard(force: bool) -> None:
    games, builders = in_the_way()
    if games:
        raise SystemExit(f"refusing: {games} already running, and two games share one GPU. Close it first.")
    if builders and not force:
        raise SystemExit(
            f"refusing: {builders} running, and a frame measured beside a build is a frame measured "
            "beside a build. Wait for it, or --force and read the numbers as such."
        )


def session(args: argparse.Namespace, sides: dict[str, Path]) -> Path:
    """Run every route on every side for every round, and write it all down."""
    guard(args.force)
    for exe in sides.values():
        if not exe.exists():
            raise SystemExit(f"no binary at {exe}: cargo build --release -p freeport_app")
    names = pick(args.scenarios)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else OUT_ROOT / f"{stamp}-{args.label}"
    (out / "runs").mkdir(parents=True, exist_ok=True)
    binaries = {side: binary(exe) for side, exe in sides.items()}
    for side, b in binaries.items():
        if not b["has_bot"]:
            raise SystemExit(f"the {side} binary {b['path']} has no bot in it (no --bot-report): build one that does")
    meta = {
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "label": args.label, "rounds": args.rounds, "scenarios": names,
        "extra_args": args.extra, "machine": machine(), "git": git_state(),
        "binaries": binaries,
    }
    say_meta(meta)
    (out / "meta.json").write_text(json.dumps(meta, indent=2))
    runs: list[dict] = []
    order = list(sides)
    for k in range(1, args.rounds + 1):
        # ABBA: the side that goes first alternates, so drift is shared.
        this = order if k % 2 else order[::-1]
        for name in names:
            for side in this:
                stem = f"{name}-{side}-r{k:02d}"
                print(f"round {k}/{args.rounds}  {name:<12} {side:<5}", end="", flush=True)
                run = run_one(sides[side], name, out / "runs" / f"{stem}.json", out / "runs" / f"{stem}.log",
                              args.timeout, args.extra)
                run.update(side=side, round=k)
                rep = load(run["report"])
                if rep:
                    run["gpu"] = rep.get("gpu")
                    print(f"  {rep.get('outcome')}: p50 {fmt(dig(rep, 'frames.all.p50_ms'))} ms"
                          f"  p99 {fmt(dig(rep, 'frames.all.p99_ms'))} ms"
                          f"  worst {fmt(dig(rep, 'frames.all.max_ms'))} ms  ({run['wall_s']:.0f} s)")
                else:
                    print(f"  FAILED (exit {run['exit_code']}, see {stem}.log)")
                runs.append(run)
                (out / "runs.json").write_text(json.dumps(runs, indent=2))
                time.sleep(args.cooldown)
    summary = summarise(runs, meta["machine"].get("gpus", []))
    (out / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    report = f"# bench {out.name}\n\n{describe(meta)}\n\n{table(summary)}"
    if len(sides) == 2:
        report += "\n## base against head\n\n" + comparison(summary, summary, "base", "head")
    (out / "report.md").write_text(report, encoding="utf-8")
    return out


def say_meta(meta: dict) -> None:
    m = meta["machine"]
    print(f"{m.get('cpu', '?')}, {', '.join(m.get('gpus', [])) or '?'}")
    if m.get("on_battery"):
        print("WARNING: on battery. A laptop on battery clocks down and every number below is a battery number.")
    for side, b in meta["binaries"].items():
        if b["older_than_source"]:
            print(f"WARNING: the {side} binary is older than the source it was built from")


def describe(meta: dict) -> str:
    m, g = meta["machine"], meta["git"]
    lines = [
        f"- machine: {m.get('cpu', '?')}; {', '.join(m.get('gpus', [])) or '?'}; "
        f"{m.get('memory_gb', '?')} GB; {'battery' if m.get('on_battery') else 'mains'}; {m.get('power_scheme', '')}",
        f"- source: {g['branch']} {g['commit'][:10]}{' (dirty)' if g['dirty'] else ''}",
    ]
    for side, b in meta["binaries"].items():
        lines.append(f"- {side}: {b['path']} sha256 {b['sha256'][:12]} built {b['built_utc']}")
    lines.append(f"- {meta['rounds']} rounds of {', '.join(meta['scenarios'])}; extra args {meta['extra_args'] or 'none'}")
    return "\n".join(lines)


def spike_report(path: Path, top: int) -> str:
    """The worst frames of one run, each with what the bot was doing and
    where the time went. A frame whose update is most of it was the main
    thread's; one whose update is a sliver of it waited on the GPU or the
    presentation, which no amount of work on the systems will move."""
    import csv

    with open(path, newline="", encoding="utf-8") as f:
        rows = [r for r in csv.DictReader(f) if r["label"] not in ("settle", "done")]
    if not rows:
        return f"{path}: no frames past the settle"
    walls = sorted(float(r["wall_ms"]) for r in rows)
    lines = [
        f"{path.name}: {len(rows)} frames, p50 {walls[len(walls) // 2]:.2f} ms, "
        f"p99 {walls[int((len(walls) - 1) * 0.99)]:.2f} ms, worst {walls[-1]:.2f} ms",
        "",
        "| frame | doing | wall ms | update ms | where | speed m/s | chunks pending |",
        "| ---: | --- | ---: | ---: | --- | ---: | ---: |",
    ]
    for r in sorted(rows, key=lambda r: -float(r["wall_ms"]))[:top]:
        wall, update = float(r["wall_ms"]), float(r["update_ms"])
        where = "main thread" if update > 0.6 * wall else "render or present"
        lines.append(
            f"| {r['frame']} | {r['label']} | {wall:.2f} | {update:.2f} | {where} | "
            f"{float(r['speed_mps']):.1f} | {r['pending']} |"
        )
    by: dict[str, list[float]] = {}
    for r in rows:
        by.setdefault(r["label"], []).append(float(r["wall_ms"]))
    lines.append("")
    for label, v in by.items():
        v.sort()
        lines.append(
            f"- {label}: {len(v)} frames, p50 {v[len(v) // 2]:.2f} ms, "
            f"p99 {v[int((len(v) - 1) * 0.99)]:.2f} ms, {sum(1 for x in v if x > 1000 / 60)} over 16.7 ms"
        )
    return "\n".join(lines)


def read_summary(path: str) -> dict:
    p = Path(path)
    p = p / "summary.json" if p.is_dir() else p
    return json.loads(p.read_text())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="name the errands")
    spikes = sub.add_parser("spikes", help="the worst frames of one run, and what the bot was doing")
    spikes.add_argument("csv")
    spikes.add_argument("--top", type=int, default=15)
    for name in ("run", "ab"):
        p = sub.add_parser(name)
        p.add_argument("--scenarios", help="comma separated, or 'all'; the defaults otherwise")
        p.add_argument("--rounds", type=int, default=3)
        p.add_argument("--timeout", type=float, help="wall seconds a round may take, else the errand's own")
        p.add_argument("--cooldown", type=float, default=5.0, help="seconds between rounds")
        p.add_argument("--label", default=name)
        p.add_argument("--out", help="directory, else target/bench/<stamp>-<label>")
        p.add_argument("--force", action="store_true", help="run beside a compiler anyway")
        p.add_argument("extra", nargs=argparse.REMAINDER, help="-- then flags for freeport_app itself")
    sub.choices["run"].add_argument("--exe", default=str(DEFAULT_EXE))
    sub.choices["run"].add_argument("--baseline", help="a previous run's directory to compare against")
    sub.choices["ab"].add_argument("--base", required=True, help="the binary before")
    sub.choices["ab"].add_argument("--head", default=str(DEFAULT_EXE), help="the binary after")
    for name in ("compare",):
        p = sub.add_parser(name)
        p.add_argument("base")
        p.add_argument("head")
    sub.add_parser("show").add_argument("dir")
    args = parser.parse_args()
    if getattr(args, "extra", None) and args.extra[0] == "--":
        args.extra = args.extra[1:]

    if args.command == "list":
        for name, s in SCENARIOS.items():
            print(f"{name:<12} {'default' if s.default else '       '}  {s.about}")
    elif args.command == "run":
        out = session(args, {"head": Path(args.exe)})
        if args.baseline:
            text = comparison(read_summary(args.baseline), read_summary(str(out)))
            with open(out / "report.md", "a", encoding="utf-8") as f:
                f.write(f"\n## against {args.baseline}\n\n{text}")
        print((out / "report.md").read_text())
        print(f"written to {out}")
    elif args.command == "ab":
        out = session(args, {"base": Path(args.base), "head": Path(args.head)})
        print((out / "report.md").read_text())
        print(f"written to {out}")
    elif args.command == "compare":
        print(comparison(read_summary(args.base), read_summary(args.head)))
    elif args.command == "spikes":
        print(spike_report(Path(args.csv), args.top))
    elif args.command == "show":
        print((Path(args.dir) / "report.md").read_text())


if __name__ == "__main__":
    main()
