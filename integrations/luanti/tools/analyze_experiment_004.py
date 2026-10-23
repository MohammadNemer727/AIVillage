"""Applies the frozen Experiment 004 decision rules to the probe and audit reports.

Usage: analyze_experiment_004.py <repo> <probe.json> <mutation-effect.json> <audit-v2.json> <audit-legacy.json>

Thresholds are copied from docs/experiment-004-preregistration.json and must not change after
the results exist.
"""

import hashlib
import json
import pathlib
import sys

MIN_FRACTION_BIRTHS_EXPRESSED = 0.10
MIN_COVERAGE = 0.80
MIN_LOCI_WITH_COVERAGE = 14


def main():
    repo = pathlib.Path(sys.argv[1])
    probes, effects, audit_v2, audit_legacy = (
        json.loads(pathlib.Path(path).read_text()) for path in sys.argv[2:6])

    loci = [{key: probe[key] for key in (
        "locus", "parameter", "measure", "expected", "observed_direction",
        "seeds_in_expected_direction", "median_relative_effect", "passes")}
        for probe in probes]
    failing = [probe["parameter"] for probe in probes if not probe["passes"]]
    part2 = {"loci": loci, "failing_loci": failing, "all_pass": not failing,
             "absolute": [{"locus": probe["locus"], "parameter": probe["parameter"],
                           "median_low": sorted(probe["low"])[len(probe["low"]) // 2],
                           "median_high": sorted(probe["high"])[len(probe["high"]) // 2]}
                          for probe in probes]}

    coverage = []
    for locus in audit_v2["loci"]:
        changed = locus["changed"]
        coverage.append({
            "locus": locus["locus"],
            "value_changed": changed,
            "parameter_changed": locus["expressed_parameter_changed"],
            "coverage": locus["expressed_parameter_changed"] / changed if changed else 0.0,
        })
    well_covered = [row for row in coverage if row["coverage"] >= MIN_COVERAGE]
    part3 = {
        "births": audit_v2["births"],
        "fraction_births_expressed_v2": audit_v2["fraction_expressed_parameter_changed"],
        "fraction_births_expressed_legacy": audit_legacy["fraction_expressed_parameter_changed"],
        "expected_expressed_changes_in_54_births_v2":
            audit_v2["expected_expressed_changes_in_54_births"],
        "probability_none_in_54_births_v2": audit_v2["probability_none_in_54_births"],
        "coverage_by_locus": coverage,
        "loci_with_coverage_at_least_80_percent": len(well_covered),
        "passes": audit_v2["fraction_expressed_parameter_changed"] >= MIN_FRACTION_BIRTHS_EXPRESSED
        and len(well_covered) >= MIN_LOCI_WITH_COVERAGE,
    }

    by_locus = {}
    for effect in effects:
        by_locus.setdefault(effect["locus"], []).append(effect)
    part3["ordinary_mutation_effects"] = [
        {"locus": locus, "parameter": rows[0]["parameter"],
         "changes_measure_in_every_seed_both_directions":
             all(row["fraction_of_seeds_changed"] >= 1.0 for row in rows),
         "median_effect_down": next(r["median_effect"] for r in rows if r["step"] == -2),
         "median_effect_up": next(r["median_effect"] for r in rows if r["step"] == 2)}
        for locus, rows in sorted(by_locus.items())]
    part3["loci_an_ordinary_mutation_always_moves"] = sum(
        row["changes_measure_in_every_seed_both_directions"]
        for row in part3["ordinary_mutation_effects"])

    preregistration = repo / "docs/experiment-004-preregistration.json"
    report = {
        "schema_version": 1,
        "protocol": "experiment-004-genotype-to-phenotype-expression",
        "preregistration_sha256": hashlib.sha256(preregistration.read_bytes()).hexdigest(),
        "part_2_matched_probes": part2,
        "part_3_mutation_reaches_phenotype": part3,
        "conclusions": {
            "every_locus_expressed": part2["all_pass"],
            "mutation_reaches_phenotype": part3["passes"],
        },
    }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
