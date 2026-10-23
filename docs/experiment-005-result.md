# Experiment 005: embodied survival reliability

**Result date:** 2026-10-23
**Protocol:** [`experiment-005-preregistration.json`](experiment-005-preregistration.json), frozen at
commit `8db3c68` before any arm ran.
**Machine report:** [`experiment-005-result.json`](experiment-005-result.json), sha256
`49deb1db9a65a1115be76def51fc19e2670d2d25754af8f1ff5effa4e606f81f`. 48 arena arms and 4
populations, no infrastructure reruns, every arena trace replays exactly. Raw worlds stay out of git.

## Conclusion

Movement now works without a driver. **No inherited strategy helped survival, and no population
survived its first night**, so evolution still has nothing to select on. Experiment 006 stays
blocked.

| Part | Result |
| --- | --- |
| A. Turn fidelity | Fixed: 47.5 percent of commanded turn was carried out, now 100 percent (299 of 299 decisions) |
| B. Movement without a driver | **Passed**: median path 70.2 blocks against 2.5, 28 times farther, in 8 of 8 pairs |
| C. Does inheritance change survival? | **Failed**: no genotype lived longer than the stationary one |
| D. Populations through the first night | **Failed**: all four went extinct, none suitable for selection |

## A and B: movement

The dropped turns were a stale client yaw overwriting the body's write. The body now holds its
intended heading and re-asserts it, and a regression test (`ci/turn-diag.sh`) fails below 98 percent.
With the locomotor-tone gene high, a founder walks 62 to 71 blocks in a minute with no wander driver,
against 2.4 to 2.6 for a founder with a low tone. So heritable variation in movement now exists
as a real, large phenotype.

## C: a hostile encounter

One zombie 10 blocks away at night, 120 s maximum, 8 seeds, paired by seed. Decisions survived (1200 is
the cap):

| Genotype | Mean | Median | Alive at 120 s | Seeds longer than still |
| --- | ---: | ---: | ---: | ---: |
| still (no drives) | **478** | 445 | 0 of 8 | n/a |
| tone (walks when fed) | 278 | 179 | 0 of 8 | 3 |
| flight (runs when hurt) | 262 | 199 | 0 of 8 | 3 |
| runner (tone and strong thrust) | 409 | 375 | 0 of 8 | 3 |

The preregistered test needed a genotype to survive at least 25 percent longer than "still" in at
least 6 of 8 seeds. None came close; the moving genotypes did **worse** on average (tone −42
percent, flight −45 percent). No body was alive at 120 s in any arm, and the shortest lives (about 120
decisions) occurred in every genotype.

This corrects my own pilot reading. A single development run had a "runner" take no damage in two
minutes, which suggested that running outpaces a zombie. With eight seeds it was luck: the same genotype
died at 118, 123 and 122 decisions on other seeds. Single encounters are very noisy, which is why the
protocol was paired and repeated.

## D: first-night populations

Four founders of one genotype, capacity 6, natural mobs, mutation off, up to 1500 s:

| Population | Ended | Births | First death | Individuals alive at end |
| --- | ---: | ---: | ---: | ---: |
| 7401 still | extinct at 794 s | 10 | 11 min 44 s | 0 |
| 7401 runner | extinct at 904 s | 30 | 11 min 27 s | 0 |
| 7403 still | extinct at 706 s | 2 | 10 min 57 s | 0 |
| 7403 runner | extinct at 921 s | 2 | 11 min 01 s | 0 |

Every population lost its first body 11 to 12 minutes in, the first night, whatever it inherited,
and none had an individual alive at 1500 s. Reproductive opportunity was plentiful early (founders
had 8 to 25 opportunity windows each), but the criterion also required a survivor, and there were
none. The 30 births in one runner population came from new bodies replacing dead ones, and they died
in the same night.

## What this means

- The expression work is not the bottleneck any more: variants change behaviour (004) and movement
  is reliable (005 B).
- The bottleneck is **survival itself**. Of the available genes, none changes whether a body lives
  through a zombie encounter in the right direction, and walking or fleeing is as likely to hurt as
  help. There is currently no selection pressure that rewards any gene.
- A gene can only be selected if it can make a body live longer. The missing ingredient is a way to
  *avoid* the threat, not a way to move: some form of threat sensing and avoidance, shelter or
  daytime-only play, or a gene with a mechanism that actually evades. Which is a design decision.
- Experiment 006 (natural selection) should not run until some inherited variant demonstrably
  survives better in this world, because otherwise it can only report a null.

## Caveats

- The arena is flat with one zombie; the full world has many zombies and varied terrain, and its
  result (part D) was equally negative, but only two replicates per genotype were run.
- The genotypes tested use only drives and gains. Sensory gains (visual, chemical) and interoceptive
  gain were not tested in the encounter, and the brain's input context ignores retinal motion, so
  there is no mechanism by which the body perceives an approaching zombie and responds to it.
