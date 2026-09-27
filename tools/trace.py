#!/usr/bin/env python3
"""What a frame was spent ON: a Chrome trace from a traced build, read into
which systems cost the most and what ran inside the worst frames.

A measurement instrument, and standard library only, like `bench.py`. The
bot's own report says WHEN a frame was slow and whether the main thread or
the GPU was the slow part; this says WHICH system, which is the question a
fix starts from. It is the tool the city culling work did by hand.

    CARGO_TARGET_DIR=target/trace cargo build --release -p freeport_app \\
        --features bevy/trace_chrome,bevy/debug
    target/trace/release/freeport_app --bot-report t.json --bot-seconds 75
    python tools/trace.py trace-<stamp>.json                  # the costliest systems
    python tools/trace.py trace-<stamp>.json --worst 5        # and the five worst frames, opened up
    python tools/trace.py trace-<stamp>.json --frames 3400-3700

`bevy/debug` is what puts the systems' own NAMES on their spans; without it
they are all called `system`. A trace is BIG, about 2 MB a frame of the
port on this build, so it is streamed a line at a time and never held: a
first pass costs every span and finds the frames, a second opens up only
the frames asked for. Traced frames are slower than untraced ones (the
spans cost), so a traced number is for finding a system and never for a
commit message; the untraced `bench.py` run is that.

The format is `tracing-chrome`'s: one event a line, keys in alphabetical
order, a `B` where a span begins and an `E` where it ends on the same
thread, with the span's fields folded into its name (`system:
name="freeport_app::bot::steer"`).
"""
from __future__ import annotations

import argparse
import re
from collections import defaultdict
from pathlib import Path

# One begin or end event: its name, phase, process, thread and time (us).
EVENT = re.compile(rb'"name":"((?:[^"\\]|\\.)*)","ph":"([BE])","pid":(\d+),"tid":(\d+),"ts":([0-9.eE+-]+)')
# What a frame is called, in the order they are looked for: the first one
# the trace carries is the frame.
FRAMES = ("frame", "update", "main app")
# Spans shorter than this are not worth a line in a frame opened up, us.
FLOOR_US = 500.0


def short(name: bytes) -> str:
    """A span's name as a person reads it: `system: name="a::b"` as `a::b`,
    its commands as `a::b (commands)`, a schedule as `schedule Update`, and
    anything else with its empty field list taken off."""
    text = name.decode("utf-8", "replace").replace('\\"', '"')
    m = re.match(r'(system|system_commands|schedule): name="?(.*?)"?$', text)
    if not m:
        return text
    kind, what = m.groups()
    return {"system": what, "system_commands": f"{what} (commands)"}.get(kind, f"schedule {what}")


def spans(path: Path):
    """Every span in the trace as (name, thread, start us, length us),
    paired off each thread's own stack of open spans."""
    open_: dict[tuple[bytes, bytes], list[tuple[bytes, float]]] = defaultdict(list)
    with open(path, "rb") as f:
        for line in f:
            m = EVENT.search(line)
            if not m:
                continue
            name, ph, pid, tid, ts = m.groups()
            # A span with no fields is written `update: `; the name is `update`.
            if name.endswith(b": "):
                name = name[:-2]
            stack = open_[(pid, tid)]
            if ph == b"B":
                stack.append((name, float(ts)))
                continue
            # Spans nest, so the one ending is the innermost of its name;
            # anything left open inside it was cut off and is dropped.
            while stack and stack[-1][0] != name:
                stack.pop()
            if stack:
                began = stack.pop()[1]
                yield name, int(tid), began, float(ts) - began


def survey(path: Path) -> tuple[dict[bytes, list[float]], list[tuple[float, float]], str]:
    """Pass one: every span name's count, total and worst, and the frames."""
    costs: dict[bytes, list[float]] = {}
    by_name: dict[bytes, list[tuple[float, float]]] = defaultdict(list)
    wanted = {f.encode() for f in FRAMES}
    for name, _, began, length in spans(path):
        c = costs.setdefault(name, [0, 0.0, 0.0, 0.0])
        c[0] += 1
        c[1] += length
        if length > c[2]:
            c[2], c[3] = length, began
        if name in wanted:
            by_name[name].append((began, length))
    frame = next((f for f in FRAMES if by_name.get(f.encode())), "")
    return costs, sorted(by_name.get(frame.encode(), [])), frame


def costliest(costs: dict[bytes, list[float]], frames: int, top: int) -> list[str]:
    """The systems that cost the most over the run, and their worst frame."""
    lines = [
        "| span | calls | total ms | ms a frame | worst ms |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    rows = sorted(costs.items(), key=lambda kv: -kv[1][1])
    shown = 0
    for name, (n, total, worst, _) in rows:
        label = short(name)
        # The frame and the schedules are the containers; the question is
        # what is inside them.
        if label in FRAMES or name.startswith((b"schedule", b"sub app", b"multithreaded executor")):
            continue
        lines.append(f"| {label} | {n:.0f} | {total / 1e3:,.1f} | {total / 1e3 / max(frames, 1):.3f} | {worst / 1e3:.2f} |")
        shown += 1
        if shown >= top:
            break
    return lines


def opened(path: Path, windows: list[tuple[int, float, float]], top: int) -> list[str]:
    """Pass two: the costliest spans inside each frame asked for, on every
    thread, because a frame the main thread waited through was spent on
    another one."""
    inside: dict[int, list[tuple[float, str, int]]] = defaultdict(list)
    for name, tid, began, length in spans(path):
        if length < FLOOR_US:
            continue
        for k, start, end in windows:
            if start <= began < end:
                inside[k].append((length, short(name), tid))
    lines = []
    for k, start, end in windows:
        lines.append(f"\n### frame {k}: {(end - start) / 1e3:.2f} ms\n")
        lines.append("| span | thread | ms |")
        lines.append("| --- | ---: | ---: |")
        for length, label, tid in sorted(inside[k], reverse=True)[:top]:
            lines.append(f"| {label} | {tid} | {length / 1e3:.2f} |")
    return lines


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("trace")
    parser.add_argument("--top", type=int, default=25, help="spans to list")
    parser.add_argument("--worst", type=int, default=3, help="the worst frames to open up")
    parser.add_argument("--frames", help="a range of frame numbers to open up instead, as a-b")
    args = parser.parse_args()
    path = Path(args.trace)
    costs, frames, frame = survey(path)
    if not frames:
        raise SystemExit(f"{path}: no frame spans ({', '.join(FRAMES)}); was it built with bevy/trace_chrome?")
    lengths = sorted(length for _, length in frames)
    print(f"# {path.name}\n")
    print(f"{len(frames)} frames (the `{frame}` span): p50 {lengths[len(lengths) // 2] / 1e3:.2f} ms, "
          f"worst {lengths[-1] / 1e3:.2f} ms. Traced frames are slower than untraced ones.\n")
    print("\n".join(costliest(costs, len(frames), args.top)))
    if args.frames:
        a, _, b = args.frames.partition("-")
        pick = range(int(a), int(b or a) + 1)
    else:
        pick = sorted(range(len(frames)), key=lambda k: -frames[k][1])[: args.worst]
    windows = [(k, frames[k][0], frames[k][0] + frames[k][1]) for k in pick if 0 <= k < len(frames)]
    if windows:
        print("\n".join(opened(path, windows, args.top)))


if __name__ == "__main__":
    main()
