"""Applies the frozen Experiment 005E part P (perception) rules to a completed run directory.

Usage: analyze_experiment_005e_perception.py <repo> <run-directory>

The retina frames are the ordinary ones in each arm's bridge trace; ground truth comes from the
fixture. Thresholds are copied from docs/experiment-005e-preregistration.json.
"""

import hashlib
import json
import math
import pathlib
import statistics
import sys

SEEDS = [7911, 7913, 7917, 7919, 7923, 7927, 7929, 7931]
REPEAT_SEEDS = SEEDS[:2]
MODES = ("approach", "retreat", "occluded", "zombie", "blank")
POINTS, COLUMNS = 120, 24
SETTLE = 10          # decisions discarded while the body is placed and the world settles
CHANGE = 25          # occlusion units (of 255) a ray must differ from its baseline by
NEAR, FAR = 4.0, 6.5  # blocks: "in range" frames and "object out of range" baseline frames
MAX_BEARING = 86.0   # degrees: inside the retina's 24 x 7.5 degree field
GATES = {"detect": 0.90, "false_alarm": 0.02, "azimuth_error": 10.0, "nearness_rho": 0.80,
         "motion_sign": 0.80, "occluded_leak": 0.05, "repeat_correlation": 0.95, "seeds": 7}


def load(root, name):
    arm = root / name
    truth = json.loads((arm / "perception-result.json").read_text())["truth"]
    frames = [json.loads(line)["request"]["observation"]["retina"]
              for line in (arm / "bridge.ndjson").read_text().splitlines() if line]
    return truth[SETTLE:], frames[SETTLE:len(truth)]


def spearman(a, b):
    def ranks(values):
        order = sorted(range(len(values)), key=values.__getitem__)
        out = [0.0] * len(values)
        for rank, index in enumerate(order):
            out[index] = rank
        return out
    ra, rb = ranks(a), ranks(b)
    ma, mb = statistics.mean(ra), statistics.mean(rb)
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = math.sqrt(sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb))
    return num / den if den else 0.0


def pearson(a, b):
    ma, mb = statistics.mean(a), statistics.mean(b)
    num = sum((x - ma) * (y - mb) for x, y in zip(a, b))
    den = math.sqrt(sum((x - ma) ** 2 for x in a) * sum((y - mb) ** 2 for y in b))
    return num / den if den else 0.0


def cues(truth, frames):
    """Per frame: rays whose occlusion departs from that ray's baseline, and the retina rows."""
    away = [i for i, row in enumerate(truth) if row.get("distance", 99) > FAR]
    base_frames = away or list(range(len(truth)))
    baseline = [statistics.median(frames[i][j]["occlusion"] for i in base_frames)
                for j in range(POINTS)]
    out = []
    for frame in frames:
        changed = [j for j in range(POINTS) if abs(frame[j]["occlusion"] - baseline[j]) >= CHANGE]
        out.append(changed)
    return baseline, out


def describe(root, name, mode):
    truth, frames = load(root, name)
    baseline, changed = cues(truth, frames)
    in_range = [i for i, row in enumerate(truth) if row.get("distance", 99) <= NEAR
                and abs(row.get("bearing_left_degrees", 999)) <= MAX_BEARING]
    out_of_range = [i for i, row in enumerate(truth) if row.get("distance", 99) > FAR]
    result = {"name": name, "mode": mode, "in_range_frames": len(in_range)}
    if mode == "blank":
        result["false_alarm"] = sum(bool(c) for c in changed) / len(changed)
        return result
    result["false_alarm"] = (sum(bool(changed[i]) for i in out_of_range) / len(out_of_range)
                             if out_of_range else 0.0)
    result["detect"] = (sum(bool(changed[i]) for i in in_range) / len(in_range)
                        if in_range else 0.0)
    errors, strength, distance, motion_ok = [], [], [], []
    for i in in_range:
        if not changed[i]:
            continue
        columns = [j % COLUMNS for j in changed[i]]
        weights = [abs(frames[i][j]["occlusion"] - baseline[j]) for j in changed[i]]
        centre = sum(c * w for c, w in zip(columns, weights)) / sum(weights)
        errors.append(abs((centre - 11.5) * 7.5 - truth[i]["bearing_left_degrees"]))
        strength.append(max(weights))
        distance.append(-truth[i]["distance"])
        sign = statistics.mean(frames[i][j]["motion"] for j in changed[i])
        if abs(truth[i].get("closing", 0)) > 0.3:
            motion_ok.append((sign > 0) == (truth[i]["closing"] > 0))
    result["azimuth_error_median"] = statistics.median(errors) if errors else None
    result["nearness_rho"] = spearman(strength, distance) if len(strength) > 3 else None
    result["motion_sign_correct"] = sum(motion_ok) / len(motion_ok) if motion_ok else None
    result["cue_series"] = [max((abs(frames[i][j]["occlusion"] - baseline[j]) for j in changed[i]),
                                default=0) for i in range(len(frames))]
    return result


def check(rows, mode):
    ok = []
    for row in rows:
        good = row["false_alarm"] <= GATES["false_alarm"]
        if mode in ("approach", "retreat", "zombie"):
            good = (good and row["detect"] >= GATES["detect"]
                    and row["azimuth_error_median"] is not None
                    and row["azimuth_error_median"] <= GATES["azimuth_error"]
                    and row["motion_sign_correct"] is not None
                    and row["motion_sign_correct"] >= GATES["motion_sign"])
        if mode in ("approach", "retreat"):
            good = good and (row["nearness_rho"] or 0) >= GATES["nearness_rho"]
        if mode == "occluded":
            good = good and row["detect"] <= GATES["occluded_leak"]
        ok.append(good)
    return ok


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    report = {"schema_version": 1, "protocol": "experiment-005e-perception",
              "git_revision_of_snapshot": (root / "frozen/git-revision").read_text().strip(),
              "gates": GATES}
    for mode in MODES:
        rows = [describe(root, f"{mode}-{seed}", mode) for seed in SEEDS]
        flags = check(rows, mode)
        for row, flag in zip(rows, flags):
            row["passes"] = flag
            row.pop("cue_series", None) if mode == "blank" else None
        report[mode] = {"seeds_passing": sum(flags), "passes": sum(flags) >= GATES["seeds"],
                        "rows": [{k: v for k, v in row.items() if k != "cue_series"} for row in rows]}
    repeats = []
    for seed in REPEAT_SEEDS:
        a = describe(root, f"approach-{seed}", "approach")["cue_series"]
        b = describe(root, f"approach-{seed}-repeat", "approach")["cue_series"]
        repeats.append({"seed": seed, "correlation": pearson(a, b)})
    report["repeat_runs"] = repeats
    report["repeatable"] = all(r["correlation"] >= GATES["repeat_correlation"] for r in repeats)
    report["perception_gates_pass"] = (
        report["approach"]["passes"] and report["retreat"]["passes"]
        and report["occluded"]["passes"] and report["zombie"]["passes"]
        and report["blank"]["seeds_passing"] >= GATES["seeds"] and report["repeatable"])
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
