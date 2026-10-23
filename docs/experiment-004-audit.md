# Experiment 004, part 1: genome audit and mutation analysis

**Date:** 2026-10-23
**Tool:** `sim-luanti genome-audit <births>` ([source](../crates/sim-luanti/src/genome_audit.rs));
machine report for 4,000,000 simulated births:
[`experiment-004-mutation-audit.json`](experiment-004-mutation-audit.json).

The audit replicates the mutation draw only to attribute each outcome, and fails if the replica
ever differs from the real `asexual_offspring_with_mutation`. It did not differ on any of the
4,000,000 births.

## 1. Which loci reach the live runtime

A source audit of every read of every locus (`DevelopmentalGenome::locus` and the brain-config
derivation) gives:

| Locus | Live runtime | Legacy simulated world (not the live game) |
| ---: | --- | --- |
| 6 | `connection_stride = 1 + locus % 15` | none |
| 13 | `plasticity_scale = 1 + locus / 16` (values 1 to 5) | none |
| 0 | none | growth target `64 + locus` |
| 1 | none | head radius `(32 + locus / 2) / 192` |
| 12 | none | metabolic cost `locus / 128`, which is **0 for every value up to 64**, so it has no effect there either |
| 2, 3, 4, 5, 7, 8, 9, 10, 11, 14, 15 | **none, read nowhere** | none |

So **2 of 16 loci reach the live brain**, 3 more feed only the legacy simulation (and one of those
is arithmetically inert), and **11 are not read at all**. Locus 13 is also coarse: it changes the
derived parameter only when a step crosses a multiple of 16.

## 2. How mutation actually samples them

Per birth, per locus (4,000,000 births, mutation rate 12,000 ppm):

| Quantity | Measured | Expected |
| --- | ---: | ---: |
| Locus selected for mutation | 0.0120 | 0.0120 |
| Selected with a zero step | 0.200 | 0.200 |
| Applied steps −2, −1, 0, +1, +2 | flat within sampling noise | uniform |
| Nonzero step clamped back to the same value | about 580 per 4 M births per locus | rare |
| Founder locus values at the clamp limits 0 or 64 | 3.1 percent | 3.1 percent |
| Stored value changed | 0.0094 to 0.0095 for every locus | 0.0096 |

Mutation behaves as written: each locus changes on about 0.95 percent of births, by one or two
steps either way. Per birth the number of changed loci is 0 (85.9 percent), 1 (13.1), 2 (0.94),
3 (0.04). **14.1 percent of births change at least one locus.**

## 3. What reaches the phenotype

- Only **1.03 percent** of births change a live parameter. Locus 6 almost always does when it
  changes (37,928 times); locus 13 does so rarely (3,485 of about 37,800 changes) because of the
  `locus / 16` bucketing.
- That is about **one expressed change per 97 births**, so a 54-birth experiment expects **0.56**
  and sees none **57 percent of the time**.

## 4. Conclusion for the plan

Experiment 002's "0 of 54 births changed an expressed parameter" does **not** show the mutation
system is broken. It is the expected outcome of two facts: mutation is rare (1.2 percent per
locus) and small (one or two of 64 steps), and almost nothing it changes is read. Fixing it is a
question of **expression**, not of the mutation code. That points the rest of Experiment 004 at:

1. giving every locus a bounded, interpretable role in the live runtime, so about 14 percent of
   births change something that matters rather than about 1 percent;
2. checking that a one-step change is large enough, in the parameter it drives, to produce a
   measurable behavioural difference, since mutation steps are only 1.5 percent of a locus's range;
3. only then asking whether selection can act on it (Experiment 006).
