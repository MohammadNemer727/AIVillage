"""Applies the frozen Experiment 002 decision rules to a completed run directory.

Usage: analyze_experiment_002.py <repo> <run-directory>

Thresholds are copied from docs/experiment-002-preregistration.json and must not change after
outcomes exist.
"""

import hashlib
import itertools
import json
import os
import pathlib
import random
import re
import subprocess
import sys

GENOTYPES = 4
EVOLUTION = [f"evolution-{replicate}" for replicate in range(8)]
GARDEN = [f"garden-{block}" for block in range(4)]
PHENOTYPES = ("P1_lifespan_decisions", "P2_reproductive_signal_rate", "P3_locomotor_activity")
H1_ALPHA = 0.05 / 3
H2_ALPHA = 0.05
H2_MIN_INFORMATIVE = 6
H2_SHUFFLES = 20000
H2_RNG_SEED = 20261023


def lineage(root, run):
    report = root / run / "population-report.json"
    rotation = json.loads(report.read_text()).get("founder_genome_rotation", 0)
    output = subprocess.check_output(
        [str(root / "frozen/sim-luanti"), "lineage", str(rotation), str(report)],
        env={**os.environ, "SIM_LUANTI_SEED": "103", "SIM_LUANTI_FOUNDER_COUNT": "4",
             "SIM_LUANTI_AGENT_ID": "1"},
        text=True,
    )
    return json.loads(output)


def server_summary(root, run):
    log = (root / run / "server.log").read_text(errors="replace")
    times = re.findall(r"^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d): ACTION\[Server\]: Player founder-\d+ died",
                       log, re.MULTILINE)
    return {
        "mob_attacks": len(re.findall(r'LuaEntitySAO "mobs_mc:[a-z_]+".*punched player founder', log)),
        "attacking_mobs": sorted(set(re.findall(r'LuaEntitySAO "(mobs_mc:[a-z_]+)".*punched player founder',
                                                log))),
        "player_deaths": len(times),
        "first_death": times[0] if times else None,
    }


def evolution_outcome(root, run):
    report = json.loads((root / run / "population-report.json").read_text())
    entries = lineage(root, run)
    births = [0] * GENOTYPES
    for entry in entries:
        if entry["parent_id"] is not None:
            births[entry["founder_ordinal"]] += 1
    return {
        "stop_reason": report["stop_reason"],
        "elapsed_seconds": report["elapsed_seconds"],
        "births": report["births"],
        "blocked": report["blocked"],
        "founder_genome_rotation": report.get("founder_genome_rotation", 0),
        "lineage_births_by_genotype": births,
        "max_generation": max(entry["generation"] for entry in entries),
        "expressed_mutations": sum(entry["expressed_mutation"] for entry in entries),
        "server": server_summary(root, run),
    }


def garden_phenotypes(root, run):
    report = json.loads((root / run / "population-report.json").read_text())
    # The garden permits no births; only founders carry a genotype phenotype.
    ordinal = {entry["individual_id"]: entry["founder_ordinal"]
               for entry in lineage(root, run) if entry["parent_id"] is None}
    counts = {}
    for line in (root / run / "bridge-trace.ndjson").read_text().splitlines():
        record = json.loads(line)
        agent = record["request"]["agent_id"]
        if agent not in ordinal:
            continue
        action = record["response"]["action"]
        row = counts.setdefault(agent, [0, 0, 0])
        row[0] += 1
        row[1] += (record.get("reproduction") or {}).get("signal_0", 0) > 0
        row[2] += any(action.get(motor, 0) != 0 for motor in ("thrust", "strafe", "jump"))
    phenotypes = [None] * GENOTYPES
    for agent, (transitions, signal, moving) in counts.items():
        phenotypes[ordinal[agent]] = {
            "individual_id": agent,
            "P1_lifespan_decisions": transitions,
            "P2_reproductive_signal_rate": signal / transitions,
            "P3_locomotor_activity": moving / transitions,
        }
    return {"stop_reason": report["stop_reason"], "elapsed_seconds": report["elapsed_seconds"],
            "births": report["births"], "phenotypes": phenotypes,
            "server": server_summary(root, run)}


def between_genotype_statistic(blocks):
    centred = [[value - sum(block) / len(block) for value in block] for block in blocks]
    return sum((sum(block[g] for block in centred) / len(centred)) ** 2
               for g in range(GENOTYPES))


def exact_within_block_p(blocks):
    observed = between_genotype_statistic(blocks)
    permutations = list(itertools.permutations(range(GENOTYPES)))
    as_extreme = total = 0
    for chosen in itertools.product(permutations, repeat=len(blocks) - 1):
        # Fixing the first block's labels is exact: the statistic is invariant to relabeling
        # every block by the same permutation.
        relabeled = [blocks[0]] + [[block[p[g]] for g in range(GENOTYPES)]
                                   for block, p in zip(blocks[1:], chosen)]
        total += 1
        as_extreme += between_genotype_statistic(relabeled) >= observed - 1e-12
    return observed, as_extreme / total


def average_ranks(values):
    order = sorted(range(len(values)), key=lambda index: values[index])
    ranks = [0.0] * len(values)
    index = 0
    while index < len(order):
        end = index
        while end + 1 < len(order) and values[order[end + 1]] == values[order[index]]:
            end += 1
        for position in range(index, end + 1):
            ranks[order[position]] = (index + end) / 2 + 1
        index = end + 1
    return ranks


def kendall_w(rows):
    ranks = [average_ranks(row) for row in rows]
    totals = [sum(rank[g] for rank in ranks) for g in range(GENOTYPES)]
    mean = sum(totals) / GENOTYPES
    m, n = len(rows), GENOTYPES
    return 12 * sum((total - mean) ** 2 for total in totals) / (m * m * (n ** 3 - n))


def spearman(left, right):
    a, b = average_ranks(left), average_ranks(right)
    mean_a, mean_b = sum(a) / len(a), sum(b) / len(b)
    covariance = sum((x - mean_a) * (y - mean_b) for x, y in zip(a, b))
    scale = (sum((x - mean_a) ** 2 for x in a) * sum((y - mean_b) ** 2 for y in b)) ** 0.5
    return covariance / scale if scale else None


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    evolution = {run: evolution_outcome(root, run) for run in EVOLUTION}
    garden = {run: garden_phenotypes(root, run) for run in GARDEN}

    h1 = {}
    for phenotype in PHENOTYPES:
        blocks = [[garden[run]["phenotypes"][g][phenotype] for g in range(GENOTYPES)]
                  for run in GARDEN]
        statistic, p_value = exact_within_block_p(blocks)
        h1[phenotype] = {
            "genotype_means": [sum(block[g] for block in blocks) / len(blocks)
                               for g in range(GENOTYPES)],
            "statistic": statistic,
            "p": p_value,
            "passes": p_value <= H1_ALPHA,
        }
    h1_passes = any(result["passes"] for result in h1.values())

    rows = [evolution[run]["lineage_births_by_genotype"] for run in EVOLUTION]
    informative = [row for row in rows if sum(row) > 0]
    h2 = {"informative_replicates": len(informative), "kendall_w": None, "p": None,
          "passes": False}
    if len(informative) >= 2:
        observed = kendall_w(informative)
        rng = random.Random(H2_RNG_SEED)
        as_extreme = 0
        for _ in range(H2_SHUFFLES):
            shuffled = [rng.sample(row, len(row)) for row in informative]
            as_extreme += kendall_w(shuffled) >= observed - 1e-12
        h2.update({"kendall_w": observed, "p": (as_extreme + 1) / (H2_SHUFFLES + 1)})
        h2["passes"] = len(informative) >= H2_MIN_INFORMATIVE and h2["p"] <= H2_ALPHA
    total_births = [sum(row[g] for row in rows) for g in range(GENOTYPES)]

    report = {
        "schema_version": 1,
        "protocol": "experiment-002-heritable-behavioral-variation",
        "preregistration_sha256": hashlib.sha256(
            (repo / "docs/experiment-002-preregistration.json").read_bytes()).hexdigest(),
        "git_revision_of_snapshot": (root / "frozen/git-revision").read_text().strip(),
        "infrastructure_reruns": (root / "infrastructure-reruns.txt").read_text().split()
        if (root / "infrastructure-reruns.txt").exists() else [],
        "infrastructure_blocks": sum(evolution[run]["blocked"]["infrastructure"]
                                     for run in EVOLUTION),
        "H1_repeatable_behavioural_difference": {"phenotypes": h1, "passes": h1_passes},
        "H2_consistent_lineage_response": {**h2, "total_lineage_births_by_genotype":
                                           total_births},
        "conclusion": ("selectable heritable behavioural variation demonstrated"
                       if h1_passes and h2["passes"] else "not demonstrated"),
        "secondary": {
            "spearman_garden_P2_vs_lineage_births": spearman(
                h1["P2_reproductive_signal_rate"]["genotype_means"], total_births),
            "expressed_mutations": sum(evolution[run]["expressed_mutations"]
                                       for run in EVOLUTION),
            "max_generation": max(evolution[run]["max_generation"] for run in EVOLUTION),
        },
        "evolution": evolution,
        "garden": garden,
    }
    (root / "experiment-002-result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in (
        "H1_repeatable_behavioural_difference", "H2_consistent_lineage_response",
        "conclusion", "secondary", "infrastructure_blocks")}, indent=2))


if __name__ == "__main__":
    main()
