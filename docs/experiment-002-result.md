# Experiment 002: heritable behavioural variation

**Result date:** 2026-10-23
**Protocol:** [`experiment-002-preregistration.json`](experiment-002-preregistration.json) with
[`amendment-1`](experiment-002-amendment-1.json), frozen before any outcome existed.
**Machine report:** [`experiment-002-result.json`](experiment-002-result.json), sha256
`a0c6d79a1da5d560a40393a95e47b35626872e5176f48023048399de1f8fdf0a`, from the snapshot of revision
`e2976fb`. Raw worlds and traces stay out of git.

## Conclusion

**Selectable heritable behavioural variation is not demonstrated.** The preregistered rule needed
both parts to pass; one did.

| Part | Result |
| --- | --- |
| H1 repeatable behavioural difference between founder genotypes (common garden, 4 Latin-square blocks) | **Passed on one phenotype of three** |
| H2 consistent lineage response across 8 independent populations | **Failed** (Kendall W 0.15, p 0.26) |

No run needed an infrastructure rerun, no reproduction was blocked by infrastructure, and all 12
populations ran to natural extinction (about 12–14 minutes each).

## H1: genotypes do differ, by very little

Four founder genotypes (distinct connection stride and plasticity scale) were placed in every
slot across four blocks, with mutation off and every individual a newborn.

| Phenotype | Genotype 0 | Genotype 1 | Genotype 2 | Genotype 3 | Exact permutation p | Passes (≤ 0.0167) |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Lifespan, decisions | 2277.5 | 2203.8 | 2237.8 | 2247.3 | 0.783 | no |
| Reproductive-signal rate | 0.939 | 0.971 | 0.939 | 0.965 | 0.056 | no |
| Locomotor activity (fraction of decisions) | 0.0146 | 0.0050 | 0.0081 | 0.0085 | 0.0003 | **yes** |

Genotype 0 (connection stride 5, plasticity scale 1) moved most in every block, so the effect is
repeatable. It is also tiny: even the most active genotype locomoted on 1.5 percent of decisions,
the least on 0.5 percent. Baseline founders are almost immobile, so this is a statistically clear
difference in a behaviour that is, in absolute terms, nearly absent. It is not visible play.

## H2: lineage success does not follow genotype consistently

Lineage births per founder genotype, by replicate (each replicate rotated the genomes across body
slots, so genotype is not confounded with slot):

| Replicate | 0 | 1 | 2 | 3 |
| --- | ---: | ---: | ---: | ---: |
| evolution-0 | 0 | 5 | 0 | 0 |
| evolution-1 | 9 | 0 | 1 | 1 |
| evolution-2 | 2 | 0 | 4 | 0 |
| evolution-3 | 3 | 0 | 1 | 0 |
| evolution-4 | 1 | 0 | 1 | 2 |
| evolution-5 | 1 | 0 | 0 | 8 |
| evolution-6 | 1 | 5 | 0 | 0 |
| evolution-7 | 7 | 0 | 2 | 0 |

Genotype 0 has the most births overall (24 against 10, 9 and 11), but each replicate is dominated
by whichever lineage happened to reproduce, and which lineage that is changes from replicate to
replicate. The preregistered permutation test finds no more agreement than chance.

## What else the data say

- **Mutation contributed nothing.** Across 54 births there were **zero** births with a changed
  expressed parameter. Only two of the sixteen genome loci are expressed in the live brain and each
  locus mutates with probability 0.012, so this was expected. With this genome, variation comes only
  from the founders.
- **Survival was set by the first night.** In every run the first death came 11–12 minutes after the
  world started, and bodies took on average 90 zombie attacks per run. Births mostly came from
  founders in the first minutes and the resulting newborns died within seconds. This confirms the
  addendum to Experiment 001 and bounds what any generational experiment can show in this world.
- The Spearman correlation between a genotype's reproductive-signal rate and its lineage births was
  −0.4, which is reported only as a secondary figure.

## Consequences for the plan

- The plan's goal for this stage was *selectable heritable behavioural variation*. The evidence is
  that heritable variation in a measured behaviour exists but is negligible and has no consistent
  consequence for reproduction. It does not support claiming adaptation, and it does not unblock
  anything: evolved-learning and cultural-evolution questions depend on survival that works past the
  first night and on variation in expressed parameters that actually matters to behaviour.
- A stronger test needs (1) a world in which individuals survive long enough to reproduce on their
  merits (shelter or daytime-only worlds), and (2) a genome that expresses more than two loci.
  Neither exists yet, and neither is claimed here.
