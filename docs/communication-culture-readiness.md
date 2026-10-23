# Communication, culture and evolved learning: why they are not built

**Date:** 2026-10-23

The plan's last three stages are physical communication, founder-removal and cumulative culture,
and the evolved-learning benchmark. None of them has been built or run. This note records what each
needs, what the measurements so far say about whether it can be tested here, and the smallest
steps that would change that. It makes no claim about any of them.

## What the evidence from Experiments 001 and 002 shows

| Measured fact | Source |
| --- | --- |
| Every individual in every generational run died at the first VoxeLibre night, 11–12 minutes after the world started; bodies took about 90 zombie attacks per run | Experiments 001 and 002 server logs |
| Newborns that took a dead body's slot appeared among the same zombies and died within about 40 seconds, even when fully fed | Experiment 001 addendum |
| The deepest lineage reached generation 3; 54 births across eight populations | Experiment 002 |
| Baseline founders locomote on 0.5–1.5 percent of decisions; the curiosity policy often never moves at all | Experiment 002 garden, 8C development pilots |
| Only 2 of the 16 genome loci reach the live brain, and none of the 54 births changed an expressed one | Experiment 002 lineage reconstruction |

## Stage by stage

### Physical communication (spatial acoustic signalling)

What exists: an `acoustic` motor output that only selects an animation label, and a `vibration`
sense that is a permanently silenced channel. There is no propagation, no cost, no direction or
intensity at the receiver, and no test of any of it.

What it needs: Lua-side emission that costs the sender, local direction and intensity delivered
through a real sense, no sender identity or token, and sender-absent and sham controls.

Why it cannot be tested now: a signal can only carry information if there is something worth
signalling and bodies live long enough to learn the association. Founders barely move, and a
lineage's whole history is under 15 minutes.

### Artifacts, founder-removal and cumulative culture

What exists: the body can dig and place blocks (the founder body test exercises both).

What it needs, in order: individuals that build useful structures on their own (nothing does), a
way for a descendant to perceive them as ordinary world, controls with artifacts erased, shuffled
and sham-matched, and descendants that live long enough after the founders are removed to show
faster recovery of a useful behaviour. The cumulative-culture gate then needs several generations
improving an inherited artifact.

Why it cannot be tested now: the founder-removal gate compares descendants with and without
inherited artifacts, but no individual produces artifacts, and descendants die within seconds of
birth. A test run today could only come out null for reasons that say nothing about culture.

### Evolved-learning benchmark

What exists: a common-garden harness (Experiment 002) that places newborn individuals of chosen
genotypes in matched worlds, and exact lineage reconstruction.

What it needs: ancestral and descendant genomes that differ in what the brain expresses, then
matched fresh lifetimes in unfamiliar worlds, with within-life improvement measured separately from
newborn behaviour.

Why it cannot be tested now: no descendant genome differs from its ancestor in an expressed
parameter (0 of 54 births), so ancestral and descendant individuals are the same individual.
Mutation as implemented cannot create the variation the benchmark compares.

## The smallest steps that would unblock this work

1. **Survive the first night.** Give individuals a way through it: a day-only world, a protected
   spawn, or enough behaviour to avoid zombies. Until then every generational result measures
   nightfall.
2. **Make founders explore.** An exploratory gait exists only as the fixed `wander` test driver.
   A learned or evolved source of locomotion is needed before building, signalling or revisiting
   places is a behaviour the founders show rather than one a driver imposes.
3. **Express more of the genome.** Either more loci that shape the brain or a higher mutation rate
   on the expressed ones, so descendants can differ from ancestors.
4. **Only then** build the acoustic channel and the artifact controls, and write their protocols
   before any run.

## Predictor robustness (a standing requirement in the plan)

The plan lists four separate robustness checks for the recurrent predictor: within-life
improvement, behaviour in unfamiliar situations, stability over longer and more varied
experience, and checkpoint stability across continued learning. None has been run. The held-out
margins remain modest (vision MAE 26.5 against 28.2 for persistence).
