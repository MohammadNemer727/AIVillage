# Evolving VoxeLibre population: where the plan stands

**Date:** 2026-10-23. Every claim below links to the report that supports it. Nothing here goes
beyond those reports.

## Summary

The plan's experiments were run in its fixed order, each from a protocol frozen before its data
existed. Three stages are complete with a decision; the last three are not built, and the reasons
are measured, not assumed.

| Plan stage | Outcome | Report |
| --- | --- | --- |
| Population, persistence, lifecycle, Experiment 001 | Done earlier | [experiment-001-result.md](experiment-001-result.md) |
| Embodied interface, recurrent predictor, episodic memory implementation | Done earlier | [episodic-memory.md](episodic-memory.md) |
| **Experiment 003**: episodic integration | **Policy-only retrieval accepted; prediction-integrated retrieval rejected** | [experiment-003-result.md](experiment-003-result.md) |
| **Experiment 002**: heritable behavioural variation | **Not demonstrated** | [experiment-002-result.md](experiment-002-result.md) |
| **8B**: semantic graph consolidation | **Accepted, narrowly, for one fixture** | [semantic-graph-8b-result.md](semantic-graph-8b-result.md) |
| **8C**: spatial memory and navigation | **Rejected under the frozen rules** (large effect, two narrow criteria failed) | [spatial-8c-result.md](spatial-8c-result.md) |
| Physical communication, founder-removal, cumulative culture | Not built; blocked | [communication-culture-readiness.md](communication-culture-readiness.md) |
| Evolved-learning benchmark | Not built; blocked | [communication-culture-readiness.md](communication-culture-readiness.md) |
| Predictor robustness (standing requirement) | Not run | [communication-culture-readiness.md](communication-culture-readiness.md) |

## What the evidence supports

- **Retrieval of private bodily consequences changes behaviour causally.** On 16 untouched pairs
  across two protocols it removed nearly all repeated harmful choices (152 → 4 losses over 8 seeds
  in Experiment 003). This is a controlled two-action fixture, not general foraging.
- **Retrieval must not feed prediction.** Body prediction got worse on every one of two untouched
  seed sets (MAE 3.46 → 4.91 on the second), so that path stays off.
- **Consolidation helps when the world changes.** After a swap of the two actions' consequences, raw
  retrieval did worse than no memory (95 against 153 post-swap returns); the graph recovered (227).
  It matched raw retrieval in a stable world.
- **Spatial memory helps a body get back to relief, but not safely enough to pass.** 3890 against
  2848 fed decisions (11 of 12 pairs better, none worse), with two entries into a harm patch the
  control never touched, and two seeds where the sham map did better.
- **Heritable variation, as built, does not matter.** Founder genotypes differ repeatably in how
  much they walk (1.5 against 0.5 percent of decisions), and that does not predict lineage success.
  No birth in 54 changed an expressed parameter.

## What the evidence does not support

- Adaptation, intelligence, or evolved capability. Nothing here shows any of them.
- That the founders explore, forage or navigate in the open game. The 8C tests used a fixed
  exploratory driver, and in the unmodified game baseline founders hardly move.
- Anything about culture. No individual builds anything, and no lineage lives past the first night.

## Findings that matter beyond the plan

- **The first VoxeLibre night ends every generational run** (11–12 minutes in, about 90 zombie
  attacks per run). Experiment 001's "natural extinction" was this.
- **The curiosity policy often never moves**, and **the episodic policy can trap a body where relief
  once occurred**: it repeated "turn in place" for a whole run because that action preceded relief.
- **The real body drops about half of commanded turns** and its decision length varies with bridge
  latency. Both would silently corrupt any dead reckoning that trusted its own commands, so the
  spatial memory reads the felt heading and the body's own clock instead.

## Process notes, including mistakes

- **Experiment 003, attempt 1** was lost to my own error (editing code while the harness rebuilt it);
  it was recorded as incomplete before any result was opened, with fresh seeds.
- **Experiment 002, attempt 1** ran with a dangling mod symlink and was aborted before any outcome;
  a liveness check now catches that within minutes.
- **8B and 8C were piloted and changed before freezing.** Those seeds are listed in each
  preregistration as consumed. Two of the changes fixed real faults the pilots exposed (a graph
  that never merged, a sham control that mapped the birth patch onto itself).
- **Thresholds in 8B and 8C were set after their pilots** and before any preregistered seed ran. 8B
  passes exactly on its positive-pair threshold.

## Code health

All gates pass on the final tree: `cargo fmt`, clippy with pedantic lints denied, 181 tests, rustdoc
with warnings denied, the shader and palette checks, the headless Luanti smoke with exact replay,
and the VoxeLibre body test. The `sim-app` GUI test binary cannot link on this machine because the
Wayland client library is missing, so its tests were not run here. An independent review found 14
issues; 11 were fixed. Still open: lifecycle commands clone the whole population and the event logs
grow without bound (fine at 4 bodies, not at scale), and `init.lua` is about 1,900 lines and should
be split.

## What would move the plan forward

1. Get individuals through the first night, so lineages can be tested on their merits.
2. A source of exploration that is learned or evolved rather than the fixed `wander` driver.
3. More of the genome reaching the brain, so descendants can differ from ancestors.
4. For spatial memory, harm-aware routing, then a repeat of 8C with a new protocol and new seeds.
5. Only then, the acoustic channel, artifacts, and founder-removal, each with a protocol written
   before any run.
