# Brain component plan

Status: brain architecture direction, 2026-10-22. The authoritative population, inheritance,
identity and experiment boundaries are in
[`evolving-population.md`](evolving-population.md). Open brain implementation decisions are
listed at the end.

## Why a new brain

The founder now has a full body: a real VoxeLibre player that sees (16 eye rays and a 24×5
retina), feels (health, fullness, air, pain), and acts (walk, turn, look, jump, dig and punch,
use and place, take the next item, eat). It is saved between sessions and covered by
`integrations/luanti/ci/body-test.sh`.

Its brain has not kept up. The curiosity learner that drives it counts which of 14 moves tends to
change its senses. It learns what its actions do, which our infant tests measure, but it cannot
learn what anything *is*. It has no notion of a zombie, food, a place or a cause, keeps no
memory beyond those counts, and ignores what its body feels. In the game it mostly wanders.
Research agents that visibly learn to forage or survive (Yoshida et al. 2024, EVAAA, Dreamer and
C-TeC in Crafter) all learn their own representation of the world from raw senses. The founder
needs the same, without breaking the project's rule: intelligence is grown from its own
experience, never transferred (no pretrained models, labels, language models or task rewards).

## Principles the design must keep

1. **Grown, not transferred.** A newborn starts only from genome-derived developmental
   parameters, architecture and reflexes, then learns from its own senses and actions. It never
   receives a parent's acquired weights or memories, pretrained models, labels or world
   knowledge; see the newborn contract in
   [`evolving-population.md`](evolving-population.md).
2. **Driven by its own body.** The only "reward" is the founder's own drive: the fall of its felt
   hunger and injury (homeostasis), plus curiosity. Nothing rewards a task.
3. **Judged by what it visibly does.** Progress is measured in the game against a random founder
   and the current curiosity founder, in numbers you can watch: how long it survives, meals per
   hour, damage taken per hour, ground explored.
4. **Reproducible.** Given a saved brain, every decision can be replayed exactly. Training is
   deterministic on one machine. The body test and the infant tests keep passing.

## The design

### 1. A world model that learns to see and predict (the core)

A small Dreamer-style recurrent world model (RSSM), trained only on the founder's own
experience:

- **Input each decision:** the retina and eye rays (colour, light, nearness, motion), body sense
  (eye pitch, ground, vertical motion, touch, acceleration, heading), what the body feels
  (health, fullness, air, pain), and the action just taken.
- **It learns** a compact internal state that summarises what is around it and what has been
  happening, by predicting its next senses and feelings from its state and action. Nothing tells
  it what to represent; whatever helps prediction is learned, which is where "things", "places"
  and "danger" can emerge.
- **Size**, chosen for this machine (22 CPU threads, no discrete GPU): a recurrent state of about
  256 units and a categorical latent of 16×16, with small encoders and decoders, for roughly 1–3
  million parameters. That is small enough to train on the CPU alongside the game.

### 2. Learning what is good for the body

- **Drive:** the distance of felt fullness and health from full, as the parked homeostatic
  learner already computes (`crates/sim-core/src/runtime/homeostasis.rs`). Reward is a fall in
  drive: eating when hungry and healing are good, and pain is bad.
- **Curiosity:** a bonus where the world model is still learning (disagreement between a few
  predictor heads, or learning progress), so it keeps exploring when fed and safe.
- **Actor and critic** trained on imagined futures inside the world model, as Dreamer does: the
  founder rehearses what its actions would lead to. This is its "dreaming", and it multiplies
  the value of real experience.
- **Reflexes kept from birth:** none are needed for eating now that it holds food, but the
  rooting reflex remains available if early learning stalls.

### 3. Later: a map it builds itself

Once the learned internal state is good, the founder builds its own graph on top of it:

- **Nodes:** places, kinds of things and body states it keeps returning to, found by clustering
  its own internal states.
- **Edges:** what it has seen lead where, and what came before pain or relief.
- **Use:** memory and planning, such as reaching a known food place when hungry.

No node has a name given by us. The parked "learned scenes with planning" experiment (see
`docs/prior-art.md`, Parked learner experiments) failed in the game because its scenes came from
raw senses and mostly encoded the nearest wall. Building the graph on a learned state is the fix.

## Getting enough experience

The founder's body is a real player, so the game runs at real time and cannot be sped up. At 10
decisions a second that is 36,000 decisions an hour, about 860,000 a day. DreamerV3 learns
Crafter's survival skills in about a million steps. So the budget is days, not hours, and is met
in four ways:

1. **Run unattended.** A headless world (as in the body test) runs day and night with nobody
   watching; you can join it at any time with `/founder` to see what it has learned.
2. **Dreaming.** Imagined rollouts in the world model give many training updates per real
   decision.
3. **Several individuals at once.** Canonical individuals gather only their own lived experience.
   Parallel experimental bodies run only in reset clones, and clone state never returns to the
   canonical population.
4. **Reset-clone fixtures.** Synthetic harnesses and reset game clones test early sensorimotor
   learning without becoming inherited experience or a second persistent population.

## Architecture changes

- **A learner process beside the bridge.** The bridge keeps answering the game within its time
  limit using the current policy, and streams experience to a learner thread that trains the
  world model, critic and actor and hands back new weights at fixed points. The game never waits
  for training.
- **Experience store:** a bounded replay buffer of recent decisions on disk in the world folder,
  saved with the brain.
- **Checkpoints:** the saved brain grows from the current JSON state to weights plus optimiser
  state plus buffer, in versioned files.
- **Action set:** the implemented 14 body actions, including use and next item, each held for a
  decision.
- **Seeing what it learned:** the observer HUD shows what the world model predicts the founder
  will see next beside what it actually sees, plus its drive and value. Its understanding then
  becomes something you can watch.
- **Framework:** to decide (see below). The recommended option is [Burn](https://burn.dev), a
  Rust deep-learning framework with automatic differentiation and a CPU backend, so the brain
  stays in the Rust workspace and keeps the bridge's determinism.

## Milestones and how each is judged

| Milestone | Done when |
| --- | --- |
| **M0. Learning infrastructure** | The learner trains beside the live game without slowing decisions; brains with weights save and resume exactly; a saved brain's decisions replay exactly; the body test and infant tests still pass. |
| **M1. It learns to predict** | Its prediction of the next senses and feelings beats simple baselines (repeating the last frame, the average frame) on held-out game experience, and the HUD shows recognisable predictions. |
| **M2. It learns to survive** | Over equal hours in the same world, against a random founder and the curiosity founder: it survives longer, eats more often when hungry, and takes less damage, by margins clear enough to see while watching, not only in averages. |
| **M3. It remembers and plans** | With its self-built map it returns to a food place it found earlier when hungry, and avoids a place where it was hurt, more often than without the map. |

Each milestone ends with a short written result in `docs/`, including results that do not work.

## Risks

- **Too slow to learn in real time.** Mitigated by dreaming, concurrent canonical individuals
  and repeatable reset-clone fixtures. Clone experience remains experimental evidence and never
  becomes canonical inherited or lifetime state.
- **CPU-bound training competes with the game.** Training gets a fixed thread budget, and
  decisions are served from the last good weights.
- **Learns the wrong things.** Prediction from colours might fixate on sky and ground. The HUD
  predictions and per-sense errors expose this early.
- **Floating-point replay.** Exact replay holds on one machine and build; across machines it is
  not promised.

## Implemented baseline

The first recurrent milestone uses a small in-tree deterministic fixed-point model rather than a
framework: a 16-value gated recurrent reservoir and lifetime-trained next-sense decoder. Its exact
sequence, replay, initialization, checkpoint, and held-out evaluation contracts are documented in
[`lifetime-predictor.md`](lifetime-predictor.md). This is a compact predictive-representation
baseline, not the larger Dreamer/RSSM design above; that remains contingent on this baseline
beating both preregistered held-out baselines.

The next bounded milestone adds private lifetime episodic records over this representation,
documented in [`episodic-memory.md`](episodic-memory.md). The implementation has fixed capacity,
versioned keys and retained source evidence, exact checkpoint/restart, newborn clearing, and
separate prediction/policy retrieval ablations.

After diagnostic and invalid-fixture runs, the final preregistered policy-only A/B passed all
eight untouched VoxeLibre pairs: losses fell from 173 to zero and returns rose from 147 to 320.
Prediction retrieval was disabled in both live arms, so it did not mediate the behavioral result.
The independent untouched prediction check kept vision tied but regressed body MAE from 3.291 to
5.351 in seven of eight traces. The episodic policy criterion passes, but the full episodic gate
remains failed. Semantic, spatial, graph, communication and cultural-memory work has not started.
