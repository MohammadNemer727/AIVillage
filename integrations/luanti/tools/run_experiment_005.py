"""Runs the preregistered Experiment 005 arms from a frozen snapshot.

Usage: run_experiment_005.py <repo> <output-directory>

Parts B and C run two real VoxeLibre arms at a time on separate ports; part D runs four-body
populations one at a time. The seeds, genotypes and durations are copied from
docs/experiment-005-preregistration.json and must not change after outcomes exist.
"""

import concurrent.futures
import os
import pathlib
import shutil
import subprocess
import sys

BASE = [32] * 16
for _drive in (3, 4, 9):
    BASE[_drive] = 0


def genome(**changes):
    loci = list(BASE)
    for locus, value in changes.items():
        loci[int(locus.removeprefix("l"))] = value
    return ",".join(str(value) for value in loci)


GENOTYPES = {
    "still": genome(),
    "tone": genome(l9=56),
    "flight": genome(l4=56),
    "runner": genome(l9=56, l0=56),
}
B_SEEDS = [7201, 7203, 7207, 7209, 7213, 7217, 7219, 7223]
C_SEEDS = [7301, 7303, 7307, 7309, 7313, 7317, 7319, 7323]
D_SEEDS = [7401, 7403]
D_DEADLINE_SECONDS = 1500
SMOKE = "--smoke" in sys.argv


def arms():
    listed = []
    for seed in B_SEEDS:
        for name, tone in (("low", 8), ("high", 56)):
            listed.append({"part": "B", "name": f"B-{seed}-{name}", "seed": seed,
                           "loci": genome(l9=tone), "decisions": 600, "zombie": 0, "night": 0})
    for seed in C_SEEDS:
        for name, loci in GENOTYPES.items():
            listed.append({"part": "C", "name": f"C-{seed}-{name}", "seed": seed, "loci": loci,
                           "decisions": 1200, "zombie": 10, "night": 1})
    return listed


def run_arm(context, arm, index):
    repo, output = context
    world = output / arm["name"]
    environment = dict(os.environ)
    environment.update({
        "EPISODIC_CHOICE_MODE": "baseline",
        "EPISODIC_CHOICE_FIXTURE": "arena",
        "EPISODIC_CHOICE_RESULT": "arena-result.json",
        "EPISODIC_CHOICE_PROTOCOL": "experiment-005-embodied-survival-reliability",
        "EPISODIC_CHOICE_BRIDGE_MOD": str(output / "frozen/ai_bridge"),
        "EPISODIC_CHOICE_FIXTURE_DIR": str(output / "frozen/arena"),
        "SIM_LUANTI_BIN": str(output / "frozen/sim-luanti"),
        "SIM_LUANTI_EXPRESSION": "v2",
        "SIM_LUANTI_FOUNDER_LOCI": arm["loci"],
        "FIXTURE_CONFIG": (f"arena_seed = {arm['seed']}\narena_decisions = {arm['decisions']}\n"
                           f"arena_zombie_distance = {arm['zombie']}\narena_night = {arm['night']}\n"
                           f"arena_arm = {arm['name']}"),
    })
    for attempt in (1, 2):
        shutil.rmtree(world, ignore_errors=True)
        subprocess.run(
            ["sh", str(repo / "integrations/luanti/ci/episodic-choice-run.sh"), str(world), "off",
             str(arm["seed"]), str(9400 + index * 2), str(32400 + index * 2)],
            env=environment, stdout=open(output / f"{arm['name']}.log", "w"),
            stderr=subprocess.STDOUT, check=False)
        if (world / "arena-result.json").exists():
            break
        (output / "infrastructure-reruns.txt").open("a").write(f"{arm['name']}\n")
        if attempt == 2:
            raise SystemExit(f"second infrastructure failure in {arm['name']}; incomplete")
    for leftover in ("map.sqlite", "home"):
        path = world / leftover
        shutil.rmtree(path, ignore_errors=True) if path.is_dir() else path.unlink(missing_ok=True)
    return arm["name"]


def run_population(repo, output, seed, name, loci):
    world = output / f"D-{seed}-{name}"
    environment = dict(os.environ)
    environment.update({
        "PLAY_BRIDGE_BIN": str(output / "frozen/sim-luanti"),
        "PLAY_AI_BRIDGE_MOD": str(output / "frozen/ai_bridge"),
        "PLAY_FOUNDER_COUNT": "4", "PLAY_POPULATION_CAPACITY": "6", "PLAY_UNATTENDED": "1",
        "PLAY_PORT": "30301", "PLAY_BRIDGE_PORT": "8782",
        "SIM_LUANTI_SEED": str(seed), "SIM_LUANTI_MODE": "baseline",
        "SIM_LUANTI_MUTATION": "disabled", "SIM_LUANTI_EXPRESSION": "v2",
        "SIM_LUANTI_FOUNDER_LOCI": ";".join([loci] * 4),
        "SIM_LUANTI_EXPERIMENT_NAME": "005-first-night-populations",
        "SIM_LUANTI_REPRODUCTIVE_MATURITY": "100",
        "SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE": "160", "SIM_LUANTI_REPRODUCTIVE_COST": "80",
        "SIM_LUANTI_EXPERIMENT_001_BIRTHS": "1000",
        "SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS": str(D_DEADLINE_SECONDS),
        "SIM_LUANTI_EXPERIMENT_001_REPORT": str(world / "population-report.json"),
        "SIM_LUANTI_TRACE": str(world / "bridge-trace.ndjson"),
    })
    for attempt in (1, 2):
        shutil.rmtree(world, ignore_errors=True)
        world.mkdir(parents=True)
        launcher = subprocess.Popen(
            ["sh", str(repo / "integrations/luanti/play.sh"), str(world)], env=environment,
            stdout=open(world / "launcher.log", "w"), stderr=subprocess.STDOUT)
        trace = world / "bridge-trace.ndjson"
        for _ in range(60):  # five minutes for the bodies to reach the bridge
            if trace.exists() and trace.stat().st_size > 0 or launcher.poll() is not None:
                break
            subprocess.run(["sleep", "5"], check=False)
        if trace.exists() and trace.stat().st_size > 0:
            launcher.wait()
        else:
            launcher.terminate()
            launcher.wait()
        if (world / "population-report.json").exists():
            for leftover in ("map.sqlite",):
                (world / leftover).unlink(missing_ok=True)
            return
        (output / "infrastructure-reruns.txt").open("a").write(f"D-{seed}-{name}\n")
        if attempt == 2:
            raise SystemExit(f"second infrastructure failure in D-{seed}-{name}; incomplete")


def main():
    global D_DEADLINE_SECONDS
    repo, output = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    listed = arms()
    if SMOKE:
        # Plumbing check only: two tiny arms and one 60-second population.
        listed = [dict(arm, decisions=60, zombie=arm["zombie"]) for arm in listed[:1] + listed[-1:]]
        D_DEADLINE_SECONDS = 60
    context = (repo, output)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        for finished in pool.map(lambda pair: run_arm(context, pair[1], pair[0]),
                                 enumerate(listed)):
            print("finished", finished, flush=True)
    for seed in D_SEEDS[:1] if SMOKE else D_SEEDS:
        for name in ("runner",) if SMOKE else ("still", "runner"):
            run_population(repo, output, seed, name, GENOTYPES[name])
            print("finished population", seed, name, flush=True)


if __name__ == "__main__":
    main()
