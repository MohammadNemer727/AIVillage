"""Runs the preregistered Experiment 005E perception arms from a frozen snapshot.

Usage: run_experiment_005e_perception.py <repo> <output-directory>

Two live VoxeLibre arms run at a time on separate ports. Seeds, modes and decisions are copied from
docs/experiment-005e-perception-preregistration.json and must not change after outcomes exist.
"""

import concurrent.futures
import os
import pathlib
import shutil
import subprocess
import sys

BODY = "32,32,32,0,0,32,32,32,32,0,32,32,32,32,32,32"  # the base genome of Experiment 005
SEEDS = [7911, 7913, 7917, 7919, 7923, 7927, 7929, 7931]
MODES = ("approach", "retreat", "occluded", "zombie", "blank")
DECISIONS = 100


def arms():
    listed = [(f"{mode}-{seed}", mode, seed) for mode in MODES for seed in SEEDS]
    listed += [(f"approach-{seed}-repeat", "approach", seed) for seed in SEEDS[:2]]
    return listed


def run_arm(context, arm, index):
    repo, output = context
    name, mode, seed = arm
    world = output / name
    environment = dict(os.environ)
    environment.update({
        "EPISODIC_CHOICE_MODE": "baseline", "EPISODIC_CHOICE_FIXTURE": "perception",
        "EPISODIC_CHOICE_RESULT": "perception-result.json",
        "EPISODIC_CHOICE_PROTOCOL": "experiment-005e-perception",
        "EPISODIC_CHOICE_BRIDGE_MOD": str(output / "frozen/ai_bridge"),
        "EPISODIC_CHOICE_FIXTURE_DIR": str(output / "frozen/perception"),
        "SIM_LUANTI_BIN": str(output / "frozen/sim-luanti"),
        "SIM_LUANTI_EXPRESSION": "v2", "SIM_LUANTI_FOUNDER_LOCI": BODY,
        "FIXTURE_CONFIG": (f"perception_seed = {seed}\nperception_decisions = {DECISIONS}\n"
                           f"perception_mode = {mode}"),
    })
    for attempt in (1, 2):
        shutil.rmtree(world, ignore_errors=True)
        subprocess.run(
            ["sh", str(repo / "integrations/luanti/ci/episodic-choice-run.sh"), str(world), "off",
             str(seed), str(9700 + index * 2), str(32700 + index * 2)],
            env=environment, stdout=open(output / f"{name}.log", "w"),
            stderr=subprocess.STDOUT, check=False)
        if (world / "perception-result.json").exists():
            break
        (output / "infrastructure-reruns.txt").open("a").write(f"{name}\n")
        if attempt == 2:
            raise SystemExit(f"second infrastructure failure in {name}; incomplete")
    for leftover in ("map.sqlite", "home"):
        path = world / leftover
        shutil.rmtree(path, ignore_errors=True) if path.is_dir() else path.unlink(missing_ok=True)
    return name


def main():
    repo, output = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    listed = arms()
    if "--smoke" in sys.argv:
        listed = listed[:1]
    context = (repo, output)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        for name in pool.map(lambda pair: run_arm(context, pair[1], pair[0]), enumerate(listed)):
            print("finished", name, flush=True)


if __name__ == "__main__":
    main()
