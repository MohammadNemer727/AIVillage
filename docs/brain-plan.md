# Plan: the founder's new brain

Status: proposed, 2026-10-22. Open decisions are listed at the end and must be settled before
milestone 1 starts.

## Why a new brain

The founder now has a full body: a real VoxeLibre player that sees (16 eye rays and a 24×5
retina), feels (health, fullness, air, pain), and acts (walk, turn, look, jump, dig and punch,
use and place, take the next item, eat). It is saved between sessions and covered by
`integrations/luanti/ci/body-test.sh`.

Its brain has not kept up. The curiosity learner that drives it counts which of 12 moves tends to
change its senses. It learns what its actions do, which our infant tests measure, but it cannot
learn what anything *is*. It has no notion of a zombie, food, a place or a cause, keeps no
memory beyond those counts, and ignores what its body feels. In the game it mostly wanders.
Research agents that visibly learn to forage or survive (Yoshida et al. 2024, EVAAA, Dreamer and
C-TeC in Crafter) all learn their own representation of the world from raw senses. The founder
needs the same, without breaking the project's rule: intelligence is grown from its own
experience, never transferred (no pretrained models, labels, language models or task rewards).

## Principles the design must keep

1. **Grown, not transferred.** Every network starts blank and learns only from the founder's own
   senses and actions. Innate structure is allowed only as a body would have it: senses, drives,
   reflexes and the shape of the brain, never knowledge about the world.
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
3. **Several founders at once.** Each founder is one more windowless client; four founders give
   four times the experience, and they start the village. Each still learns only from its own
   experience at first.
4. **A nursery (to decide).** A small, fast simulated world, like the existing synthetic
   harnesses, for the first sensorimotor learning before the game. It is the founder's own
   experience in a simpler world, not transferred knowledge, but it is a design choice for you.

## Architecture changes

- **A learner process beside the bridge.** The bridge keeps answering the game within its time
  limit using the current policy, and streams experience to a learner thread that trains the
  world model, critic and actor and hands back new weights at fixed points. The game never waits
  for training.
- **Experience store:** a bounded replay buffer of recent decisions on disk in the world folder,
  saved with the brain.
- **Checkpoints:** the saved brain grows from the current JSON state to weights plus optimiser
  state plus buffer, in versioned files.
- **Action set:** the 14 body actions (adding use and next item to today's 12), each held for a
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

- **Too slow to learn in real time.** Mitigated by dreaming, several founders and the nursery.
  If M1 needs more experience than a week of play gives, the nursery decision becomes urgent.
- **CPU-bound training competes with the game.** Training gets a fixed thread budget, and
  decisions are served from the last good weights.
- **Learns the wrong things.** Prediction from colours might fixate on sky and ground. The HUD
  predictions and per-sense errors expose this early.
- **Floating-point replay.** Exact replay holds on one machine and build; across machines it is
  not promised.

## Decisions for you

1. **Framework:** Burn (recommended), another Rust library such as Candle, or our own small
   networks written from scratch (slower to build, full control).
2. **The nursery:** allow a simple fast world for early learning, or game only.
3. **Several founders:** start with one, or run several from M0 to gather experience faster.
4. **The held item:** the founder does not yet feel what is in its hand. Add a sense for it (the
   item's colour and weight, never its name) in M0, or later.
