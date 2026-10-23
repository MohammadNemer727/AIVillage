# Experiment 004: genotype to phenotype expression

**Result date:** 2026-10-23
**Protocol:** [`experiment-004-preregistration.json`](experiment-004-preregistration.json), frozen at
commit `7c37682` before any result existed.
**Machine report:** [`experiment-004-result.json`](experiment-004-result.json), sha256
`5574bb766aded0ac3de74238ef14eba62ee5b5c49916cf5f734d153775331ee2`, with the 4,000,000-birth audits
under the [legacy](experiment-004-mutation-audit.json) and [v2](experiment-004-mutation-audit-v2.json)
maps. Part 1 (the audit) is in [experiment-004-audit.md](experiment-004-audit.md).

## Conclusion

**Every locus is now expressed, and ordinary birth mutations now reach the phenotype.** Both
preregistered parts passed. The measured effects of a single mutation are small for most loci, so
this makes selection *possible*; it does not yet show it is *likely*.

| Part | Result |
| --- | --- |
| 2. Matched one-locus-at-a-time probes, 8 seeds | **All 16 loci pass**, each in 8 of 8 seeds |
| 3. Mutation reaches the phenotype | **Passed**: 13.3 percent of births change an expressed parameter (1.03 percent before), 15 of 16 loci have at least 80 percent coverage |

## What was built

Every locus now maps, through one bounded monotone integer map, to one developmental parameter
with a direct behavioural or life-history consequence. Nothing is a behaviour; each is a gain,
threshold, drive, sensitivity or timing:

| Locus | Parameter | Applied |
| ---: | --- | --- |
| 0 | thrust gain (0.5x to 2.5x) | final action |
| 1 | turn gain (0.5x to 2.5x) | final action |
| 2 | movement threshold (0 to 192 of 256) | final action |
| 3 | hunger drive (forward drive while hungry) | final action |
| 4 | pain flight (forward drive and turn while hurt) | final action |
| 5 | jaw gain | final action |
| 6 | connection stride (unchanged) | brain |
| 7 | visual gain (0.5x to 2x) | retinal input |
| 8 | chemical gain (0.5x to 2x) | chemical receptors |
| 9 | locomotor tone (resting forward drive when fed and unhurt) | final action |
| 10 | interoceptive gain (how strongly hunger, pain and toxin are felt) | brain input |
| 11 | reserve recovery (reproductive reserve refill speed) | lifecycle |
| 12 | reproductive maturity (50 to 150 percent) | lifecycle |
| 13 | plasticity scale (unchanged) | brain |
| 14 | reserve threshold for reproducing | lifecycle |
| 15 | birth cost | lifecycle |

Expression is versioned. The legacy map is an exact no-op, and every retained trace from every earlier
experiment still replays exactly, so no earlier result is touched. A unit test confirms the map is
one-to-one: setting any single locus from 0 to 64 moves exactly its own parameter.

## Part 2: matched probes

For each locus, two genomes identical except that locus (8 against 56), the same scripted
observation stream, the real decision loop (and the real lifecycle path for life history). Medians
over 8 seeds:

| Locus | Parameter | Low arm | High arm | Relative effect |
| ---: | --- | ---: | ---: | ---: |
| 0 | mean absolute thrust | 2.9 | 14.1 | 0.80 |
| 1 | mean absolute turn | 1.7 | 4.6 | 0.63 |
| 2 | fraction of fed decisions that move | 0.021 | 0.013 | 0.42 |
| 3 | thrust while hungry | 110.7 | 153.4 | 0.28 |
| 4 | thrust while hurt | 40.5 | 224.0 | 0.82 |
| 5 | jaw amplitude | 191.8 | 255.9 | 0.25 |
| 6 | decisions whose action differs between arms | 0 | 0.185 | n/a |
| 7 | decisions that differ | 0 | 0.547 | n/a |
| 8 | decisions that differ | 0 | 0.788 | n/a |
| 9 | thrust while fed and unhurt | 12.3 | 84.2 | 0.85 |
| 10 | decisions that differ | 0 | 1.000 | n/a |
| 11 | births in 600 decisions | 10 | 35 | 0.71 |
| 12 | decision of the first birth | 61 | 136 | 0.55 |
| 13 | decisions that differ | 0 | 0.998 | n/a |
| 14 | births in 600 decisions | 21 | 18 | 0.14 |
| 15 | births in 600 decisions | 31 | 14 | 0.55 |

All 16 moved the measure the predicted way in 8 of 8 seeds. Two honest notes on size: locus 2's
absolute effect is small (about 0.8 percentage points of fed decisions), and locus 14 is the
weakest at 14 percent (21 against 18 births).

## Part 3: does mutation reach the phenotype?

- **13.3 percent of births change at least one expressed parameter** (legacy: 1.03 percent). A
  54-birth experiment now expects 7.2 expressed changes and would see none 0.04 percent of the time
  (Experiment 002 had a 57 percent chance of seeing none).
- **Coverage:** for 15 of 16 loci, every mutation that changes the stored value also changes the
  expressed parameter. The exception is locus 13 (plasticity), only 9 percent, because it keeps its
  original coarse `1 + locus / 16` map.
- **Ordinary mutations are mostly small.** A step of two from the midpoint (the largest a birth
  makes) changes the pre-declared measure by:

| Size of effect | Loci |
| --- | --- |
| 20 to 60 percent | visual gain (36 to 42), chemical gain (36 to 38), interoceptive gain (51 to 58) |
| 10 percent | connection stride |
| 1 to 6 percent | thrust gain, turn gain, hunger drive, pain flight, locomotor tone, maturity |
| not detectable in this probe | movement threshold, jaw gain (under 0.1 percent), reserve recovery (downward), plasticity (upward), reserve threshold, birth cost |

So **10 of 16 loci** have an ordinary mutation that changes behaviour by at least 1 percent.
For the other six a single mutation is invisible here, and they would have to accumulate several
steps before selection could see them. This is a property of a 12,000 ppm mutation rate with steps
of one or two out of 64, not of the expression.

## What this does and does not show

- It shows that the heritable variation can now reach behaviour and reproduction, and that
  Experiment 002's null was an expression problem as suspected.
- It does **not** show that any variant helps survival or reproduction. That is Experiment 006, and
  it depends on bodies that survive (Experiment 005).
- The probes use a scripted stream and the baseline founder, so effects are exact and deterministic
  but not an in-game outcome. A gene that raises forward drive, for instance, is shown to raise forward
  thrust; whether that helps against zombies is not tested here.
- The design changed during development, and the preregistration lists why. Three predictor-tuning
  genes and a surprise threshold were removed because they changed no prediction or action; a
  retinal-motion gene was replaced because the brain ignores retinal motion; and a zero hunger or pain
  drive was found to remove backward movement (fixed, with a regression test). Learning-machinery genes
  can return once the evolving population actually uses policy retrieval.
