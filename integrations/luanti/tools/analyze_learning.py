#!/usr/bin/env python3
"""Summarise real Luanti learning runs of the founder in the infant mobile paradigm.

Usage: analyze_learning.py REPORT.md WORLD_DIR [WORLD_DIR ...]

Each WORLD_DIR is a run made by ci/learning-run.sh. Rates are per 100 decisions.
"""

from __future__ import annotations

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

PHASES = ("baseline", "connected", "extinction")
CUBE_RGB = (176, 176, 176)
AHEAD_RAYS = (15, 0, 1)
LEARN_EVENT = re.compile(r"AI_BRIDGE_LEARN (\{.*\})")


def phase_starts(server_log: Path) -> tuple[str, dict[str, int]]:
    starts, condition = {}, "free"
    for line in server_log.read_text(errors="replace").splitlines():
        if match := LEARN_EVENT.search(line):
            event = json.loads(match[1])
            if event.get("event") == "phase":
                starts[event["phase"]] = event["sequence"]
                condition = event.get("object", condition)
    return condition, starts


def summarise(world: Path) -> tuple[str, int, dict[str, dict[str, int]]]:
    condition, starts = phase_starts(world / "server.log")
    run = json.loads((world / "run.json").read_text())
    seed = run["seed"]
    condition = f"{condition} / {run.get('learner', 'default')} / {run.get('facing', 'toward')}"
    bounds = [(name, starts[name]) for name in (*PHASES, "done") if name in starts]
    counts: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for line in (world / "bridge.ndjson").read_text().splitlines():
        record = json.loads(line)
        observation = record["request"]["observation"]
        if observation["transport"] != "Healthy":
            continue
        sequence = observation["sequence"]
        phase = next(
            (name for (name, start), (_, end) in zip(bounds, bounds[1:]) if start <= sequence < end),
            None,
        )
        if phase is None:
            continue
        action = record["response"]["action"]
        body = observation.get("body", {})
        rays = observation["senses"]["retinal"]
        row = counts[phase]
        row["decisions"] += 1
        row["pulses"] += int(action.get("hand_pulse", 0) > 0)
        row["contacts"] += int(body.get("hand_contact", 0) > 0)
        row["cube_seen"] += int(any((r["red"], r["green"], r["blue"]) == CUBE_RGB for r in rays))
        ahead = any((rays[i]["red"], rays[i]["green"], rays[i]["blue"]) == CUBE_RGB for i in AHEAD_RAYS)
        row["cube_ahead"] += int(ahead)
        row["pulses_ahead"] += int(ahead and action.get("hand_pulse", 0) > 0)
        row["looking_down"] += int(body.get("eye_pitch", 128) < 128)
    return condition, seed, counts


def rate(count: int, decisions: int) -> str:
    return f"{100 * count / decisions:.1f}" if decisions else "–"


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    report, worlds = Path(sys.argv[1]), [Path(arg) for arg in sys.argv[2:]]
    runs = [(world, *summarise(world)) for world in worlds]
    lines = [
        "# Luanti learning run",
        "",
        "Real Luanti runs of the affordance founder in the infant mobile paradigm: baseline",
        "without the test cube, a connected phase with it in front of the founder, then",
        "extinction without it. The founder and cube are returned to their places every trial.",
        "In the **free** condition a hand pulse within reach pushes the cube and reports contact;",
        "in the **inert** control the cube is present and solid but a pulse does nothing.",
        "Rates are per 100 decisions. Luanti physics is not bit-exact, so runs are not",
        "reproducible move for move, but every trace replays exactly through the Rust runtime.",
        "",
        "Reproduce one run, then summarise any set of runs:",
        "",
        "```sh",
        "SIM_LUANTI_SEED=1 LEARN_OBJECT=free sh integrations/luanti/ci/learning-run.sh /tmp/learn/free-seed1",
        "python3 integrations/luanti/tools/analyze_learning.py docs/luanti-learning-run.md /tmp/learn/*",
        "```",
        "",
        "*Hit rate* is the share of hand pulses followed by contact. *Cube ahead* is the share",
        "of decisions with the cube on the eye rays nearest straight ahead; *pulses while ahead*",
        "is the share of pulses made then. Aiming shows as pulses while ahead above cube ahead.",
        "",
        "## Pooled by condition and learner",
        "",
        "| condition / learner / facing | phase | decisions | hand pulses | hit rate % | cube ahead % | pulses while ahead % | looking down % |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    pooled: dict[str, dict[str, dict[str, int]]] = defaultdict(lambda: defaultdict(lambda: defaultdict(int)))
    for _, condition, _, counts in runs:
        for phase, row in counts.items():
            for key, value in row.items():
                pooled[condition][phase][key] += value
    for condition in sorted(pooled):
        for phase in PHASES:
            row = pooled[condition][phase]
            if not row:
                continue
            n = row["decisions"]
            lines.append(
                f"| {condition} | {phase} | {n} | {rate(row['pulses'], n)} "
                f"| {rate(row['contacts'], row['pulses'])} | {rate(row['cube_ahead'], n)} "
                f"| {rate(row['pulses_ahead'], row['pulses'])} | {rate(row['looking_down'], n)} |"
            )
    lines += [
        "",
        "## Per run (hand pulses per 100 decisions)",
        "",
        "| run | condition / learner / facing | seed | baseline | connected | extinction |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for world, condition, seed, counts in runs:
        cells = [rate(counts[p]["pulses"], counts[p]["decisions"]) for p in PHASES]
        lines.append(f"| {world.name} | {condition} | {seed} | " + " | ".join(cells) + " |")
    report.write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
