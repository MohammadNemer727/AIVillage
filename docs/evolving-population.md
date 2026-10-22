# Evolving VoxeLibre population specification

Status: Experiment 001 autonomous generational turnover implemented, 2026-10-22.

This specification extends the brain direction in [`brain-plan.md`](brain-plan.md) from one
founder to a population. Where the older plan discusses several persistent founders or an
optional nursery, this document is authoritative: the product has one canonical living world,
and controlled experiments use reset clones that cannot feed state back into it.

## Goal and experimental boundary

The goal is an evolving population living through births and deaths in **one persistent,
canonical VoxeLibre world**. The canonical world is the lineage of record: its terrain, ordinary
blocks, individuals, ancestry, learned state and history continue across server restarts.

Experiments run in **reset clones** made from an identified canonical snapshot. A clone may be
replayed, perturbed, ablated or discarded. Nothing learned, built, born or selected in a clone is
merged back into the canonical world. A result records its source snapshot, code/build identity,
seeds, policy configuration and reset procedure. This prevents benchmark optimization from
silently becoming artificial selection in the living population.

The population is not a pool of interchangeable saved brains. It consists of mortal individuals
whose inherited starting conditions can evolve while their acquired lifetime state dies with
them.

## Current implementation boundary

The foundational steps and Experiment 001 now provide deterministic inherited genomes, a Rust population
coordinator, never-reused individual IDs distinct from reusable body slots, fresh-runtime
asexual offspring, reasoned blocked/death/birth audit events, independently checkpointed live
individuals, and loopback transition/lifecycle endpoints routing a Luanti player cohort. Lua sends
both `agent_id` and `body_slot`; each body has independent sequencing, pending-request and
safe-zero state. VoxeLibre death now sends only a durable death event and leaves the slot empty.
A positive first blank policy signal from a living body is an explicit reproductive decision.
The predictive controller now writes every one of its 15 output channels: outputs 0–7 retain
their existing motor-neuron readouts and outputs 8–14 read seven distinct endogenous modulatory
neurons. Thus reproduction (output 8) is neither structurally zero nor aliased to an actuator,
and no reproductive schedule or death callback is introduced. The neural, checkpoint, and GPU
layouts are unchanged, so existing learned state keeps its meaning; exact replay output changes
only where older traces recorded the formerly unwritten signal channels as zero.
Rust independently checks configured maturity, reserve, explicit cost, and slot availability.
Successful birth atomically charges the parent, records lineage, creates a never-reused identity
with a genome-derived blank runtime, and assigns the child to the free player slot.

Lifecycle commands carry mod-storage event IDs. Rust records completed results for idempotent
retries and writes a recoverable transaction containing the post-command manifest and every live
checkpoint before acknowledging. Lua persists pending commands and synchronizes slot occupancy
and next sequence after restart. Mutation-disabled cohorts inherit the exact genome; enabled
mutation is deterministic from durable lineage values. Natural zero population is terminal.
Population-manifest schema 2 retains the exact pending reproduction output and an eligibility
snapshot for each lifecycle attempt; schema 1 remains readable and migrates with no pending
decision. Transition traces retain output signal zero, its decision tick, maturity threshold,
reserve, observed fullness, and slot availability. Lifecycle audit retains the queued attempt and
the resulting birth or explicit block, while each run report embeds an immutable source/config
provenance record. Forced-signal wiring checks, when used, are labelled and excluded from natural
pilot evidence.

This completes autonomous live rebinding for Experiment 001; it is not evidence that later staged
capability gates passed. Canonical recovery remains unimplemented, explicitly excluded from
Experiment 001 reports, and must not be confused with natural descent. Sexual recombination,
canonical/clone provenance, and controlled learning/evolution/social/cultural experiments remain
future work.

The embodied interface prerequisite for later lifetime learning is also complete. Bridge protocol
4 carries body contract 7: the selected stack reaches Rust only as bounded occupancy, rendered
colour, amount/capacity and durability, with no name, description, group, recipe or invented
weight. The active live policy selects all 14 existing body experiments, including use and
next-item. Existing 12-row learner checkpoints migrate by appending two blank rows; acquired rows
are not reinterpreted.

## Identity: body slots are not individuals

A **body slot** is an operational VoxeLibre player/avatar endpoint: the connection, current
physical body and integration resources through which an individual senses and acts. A slot can
be empty, can be reused after death, and may host many individuals sequentially. Slot names,
network identities and process IDs are never ancestry or person identity.

An **individual** has an immutable population identity assigned once at birth, plus parent IDs,
birth/death events, genome version and lineage metadata. Its lifetime brain and memories belong
to that individual even if its body is temporarily reconnected. Moving an individual between
compatible slots preserves individual identity; putting a newborn in a deceased individual's
old slot does not. Checkpoints bind individual ID and body-slot occupancy explicitly so restart
cannot turn slot reuse into resurrection.

## Three transmission paths

The following paths are separate and must remain separately measurable:

1. **Genetic transmission:** parents pass only genome-encoded developmental parameters,
   architecture parameters, reflex parameters and permitted morphology/physiology parameters,
   with recombination and mutation. Genetic transmission does not serialize learned weights,
   memories, optimizer moments or acquired map contents into the child.
2. **Social transmission:** an individual learns during its own lifetime by sensing another live
   individual's embodied behaviour and its ordinary physical consequences. There is no
   teacher-policy copy, hidden-state sharing, label channel or direct brain read.
3. **Cultural transmission:** later individuals encounter durable changes left in the common
   world, such as paths, walls, shelters, arrangements and devices made from ordinary blocks.
   These artifacts persist because the canonical VoxeLibre world persists, not because they are
   exported as lessons.

A behaviour is not called cultural merely because descendants inherited a genome that makes it
likely.

## Newborn contract

Every natural birth creates a new individual ID and a fresh lifetime state.

**Inherited and initialized from the genome**

- genome-derived initial neural/developmental parameters and architecture choices;
- genome-derived reflexes and their initial strengths;
- allowed body, sensory, drive and plasticity parameters;
- deterministic developmental seeds derived from the birth event where initialization requires
  variation.

**Empty at birth**

- all lifetime parameter deltas and learned weights beyond genome-derived initialization;
- optimizer state, moments, gradients and training counters;
- recurrent/working state;
- episodic memory and replay experience;
- learned graph/map memory;
- learned spatial state, place representations and routes.

A child may inherit a better substrate for learning, but never a parent's acquired answer.
Checkpoint loading must preserve this distinction. A newborn-state audit must reject accidental
transfer of any lifetime category.

## Reproduction, death and capacity

Reproduction is a consequence of life in the canonical world, not an evaluation operator.
Eligible living adults reproduce through world-local interactions and physiological conditions;
the runtime does **not** rank candidates by survival, benchmark score, novelty, intelligence,
reward, prediction accuracy or another metric to choose parents. Differential reproduction may
emerge from ordinary survival, access, proximity and behaviour, but no metric-based parent
selection is added behind the world.

Population limits are environmental/operational constraints, not fitness judgments. When a
valid conception or birth cannot proceed because no body slot or configured population capacity
is available, record a **capacity-blocked reproduction event** instead of silently dropping it.
Metrics include attempts, successful conceptions/births, blocked count and rate, blocking reason
(population cap, no slot, resource limit or another explicit reason), parent IDs, time and world.
Blocked rates are reported beside reproductive success because a cap can otherwise masquerade
as selection.

Death closes the individual's lifetime state and frees its slot according to the body lifecycle;
it does not erase ancestry or canonical-world artifacts.

## Canonical-only extinction recovery

Automatic extinction recovery is allowed **only in the canonical world**, only after the
population reaches zero living individuals, and only as an explicitly audited continuity
mechanism. It seeds a configured founder cohort from versioned recovery genomes; it never restores
dead individuals or lifetime deltas, optimizer, recurrent, episodic, graph or spatial state.

Each recovery records the trigger, timestamp, extinct-population summary, source genome archive,
new IDs, code/build and operator/configuration. Recovery individuals form a new cohort linked to
the recovery event, not fictitious children of the dead. Clones never auto-recover: extinction is
an experimental outcome and the clone remains extinct until reset. Recovery frequency and
post-recovery outcomes are reported separately from uninterrupted natural descent.

## Brain and memory direction

The current curiosity learner detects which actions change its senses, but in the game it mostly
wanders. The brain direction remains a small, CPU-suitable recurrent world model trained from
each individual's own raw experience:

- inputs are retina and eye rays, body senses, health, fullness, air, pain and prior action;
- prediction learning forms internal state without object labels, item names, language,
  pretrained representations or task rewards;
- homeostatic improvement and curiosity/learning progress drive action;
- a later self-built graph may organize unnamed recurring situations and places.

The first private episodic layer is now implemented over the compact recurrent predictor; see
[`episodic-memory.md`](episodic-memory.md). Its records declare the recurrent encoder version and
retain compact source observations before and after each event. Incompatible keys are rejected,
not silently reinterpreted. Newborns receive the canonical empty store, and tests inject
distinctive parental injury/relief episodes to verify that none cross birth.

After earlier diagnostic and invalid-fixture runs, the final preregistered policy-only A/B passed
all eight untouched VoxeLibre pairs: losses fell from 173 to zero and returns rose from 147 to
320. Prediction retrieval was disabled in both live policy arms, so it did not mediate that
behavioral result. The independent untouched prediction check kept vision tied but regressed body
MAE from 3.291 to 5.351 in seven of eight traces. The episodic policy criterion passes, but the
full episodic capability gate therefore remains failed. Graph, spatial, semantic, communication
and cultural-memory work has not started.

## Persistent artifacts have no semantic backchannel

Cultural artifacts use the same ordinary VoxeLibre blocks and game physics available to bodies.
The brain receives only normal visual, tactile, proprioceptive and interoceptive observations.
There is no text channel, sign parser, block-name feed, recipe lookup, semantic object API,
coordinate oracle, artifact metadata, author tag exposed to the policy, or privileged
read/write channel between brains.

Research logs may identify blocks and authors for audit, but policy input may not. If writing or
symbol use eventually emerges, marks must be physically constructed and perceived through the
ordinary sensor stream. An ablation may remove artifacts or replace them with geometry-matched
ordinary-block controls without changing the brain API.

## Replay and reproducibility boundary

Given the same recorded observation/action trace, checkpoint, runtime build and seeds, the
**Rust policy path** must replay decisions and learning updates exactly. This includes genome
development, policy RNG, mutation/recombination, newborn initialization and population-event
ordering under Rust authority.

The surrounding VoxeLibre/Luanti game is **not promised bit-exact physics replay**. Timing,
collision and engine physics may produce different world trajectories even when policy replay is
exact. Therefore:

- exact-replay claims apply to Rust over a captured trace, not to a second live game run;
- live results use repeated reset clones, multiple seeds/runs and distributions;
- traces preserve observations and actions at the bridge boundary so policy determinism can be
  distinguished from game-physics variance.

This matches [`luanti-learning-run.md`](luanti-learning-run.md), which already states that Luanti
physics is not move-for-move reproducible while Rust trace replay is exact.

## Staged capability and ablation gates

Later stages do not open merely because code exists. Each stage requires its capability gate,
negative controls and a written result in `docs/`, including failures.

| Stage | Capability gate | Required controls / ablations |
| --- | --- | --- |
| **0. Identity and lifecycle** | Canonical/clone provenance, immutable individual IDs, reusable body slots, ancestry, newborn-state audit, capacity metrics and canonical-only recovery survive save/restart tests. | Slot reuse must not preserve identity; clone extinction must not recover; newborn forbidden-state injection must be detected. |
| **1. Fresh-lifetime learning** | A newborn with all lifetime categories cleared learns a held-out sensorimotor or homeostatic contingency during one lifetime and exact Rust trace replay agrees. | Frozen learner, random policy, inherited-start-only evaluation, yoked outcomes and repeated live clone runs. |
| **2. Genetic evolution** | Across canonical generations, heritable variation changes development or fresh-lifetime learnability without copying acquired state. | Mutation off, recombination off, shuffled parentage/genomes and common-garden clone evaluation from age zero. No metric-based mate selection. |
| **3. Social learning** | A naive individual learns faster or differently after ordinary sensory exposure to a live demonstrator. | No demonstrator, non-contingent/yoked demonstrator, occlusion and demonstrator replacement; artifacts removed or reset. |
| **4. Persistent culture** | Knowledge or behaviour survives replacement of its original carriers through ordinary-block artifacts and is reacquired by naive individuals. | Fresh world, artifacts removed, geometry-matched meaningless artifacts, no live demonstrator, lineage/genome-matched cohorts and author replacement. |
| **5. Cumulative culture** | Successive cohorts improve an artifact or practice beyond one lifetime's baseline. | Genetic-only, social-only, artifact-only, shuffled-generation artifacts and reset-clone baselines; verify no semantic API or checkpoint leakage. |

The stages are ordered: genetic claims depend on a demonstrated fresh learner, social claims
require transmission controls, and cultural claims require identity/lifecycle and path
separation. Survival, meals, damage and exploration remain useful behavioural measures, but do
not replace stage-specific causal controls.

## Fresh-lifetime learnability benchmark

The central evolutionary benchmark is not an adult's accumulated score. It asks whether inherited
starting conditions make a **new, empty lifetime** better able to learn.

For each candidate generation or archived genome cohort:

1. Instantiate age-zero individuals in reset clones under the newborn contract; verify all
   lifetime deltas, optimizer, recurrent, episodic, graph and spatial state are empty.
2. Use matched worlds and counterbalanced seeds. Do not train or select parents in these clones.
3. Give each individual the same bounded experience budget on held-out
   contingencies/environments not used to decide reproduction.
4. Measure learning curves and sample efficiency: change from pre-exposure baseline, trials to a
   preregistered criterion, retention after delay/extinction and transfer to a matched novel
   case. Report failures and per-seed trajectories.
5. Compare against ancestor/archive genomes, mutation-off controls, a frozen learner and a
   random policy. Repeat with lifetime learning disabled to separate inherited competence from
   inherited learnability.

Improvement counts only if it replicates across reset clones and cannot be explained by leaked
lifetime state, changed body resources, capacity censoring or game-physics luck. These metrics
are observational; they never choose canonical parents.

## Recorded evidence and design lessons

This specification builds on experiments already recorded rather than restating their results:

- [`experiment-001-result.md`](experiment-001-result.md) records the matched mutation-enabled
  and mutation-disabled live cohorts, terminal extinction, evidence checksums, and the narrow
  conclusion that autonomous generational turnover and heredity work.
- [`contingency-test.md`](contingency-test.md) shows reward-free learning of standard, inverted,
  bimodal and seen-only contingencies with yoked controls, extinction and delayed consequences.
- [`luanti-learning-run.md`](luanti-learning-run.md) confirms a smaller contingency effect in real
  Luanti and establishes the Rust-policy/game-physics reproducibility boundary.
- [`reaching-test.md`](reaching-test.md) records that the original learner did not learn directed
  reaching, so action-outcome contingency alone is not a sufficient capability claim.
- [`affordance-benchmark.md`](affordance-benchmark.md) records deterministic baseline comparisons
  and checkpoint behaviour.
- [`prior-art.md`](prior-art.md), especially “Parked learner experiments,” records learned scenes,
  per-action situations, attention, planning and distributional variants. Learned-scene planning
  worked in a clean synthetic test but did not transfer reliably to the game because scenes often
  encoded walls and motion rather than the small cube. That motivates a learned encoder and
  staged gates; it is not evidence that semantic memory already exists.

The same audit supplies population precedents: Polyworld for genome-derived neural structure
plus lifetime plasticity, biosim4 for compact genome-to-network ideas, JaxLife for social and
cultural experiment designs, and Yoshida et al. and EVAAA for homeostatic behaviour. These are
idea and protocol sources under the reuse limits in [`prior-art.md`](prior-art.md), not imported
implementations.

## Operational risks

- **Real-time experience is slow.** At 10 decisions per second, a body gets about 860,000
  decisions per day. Unattended canonical operation and reset-clone experiments provide data,
  while clone experience remains outside canonical inheritance.
- **CPU training can disturb the game.** Learning needs a bounded compute budget and policy
  serving uses the last complete state rather than blocking the bridge.
- **Representation can learn the wrong regularities.** Held-out prediction, visual diagnostics
  and ablations must expose fixation on sky, ground, walls or slot identity.
- **Capacity can create hidden selection.** Capacity-blocked reproduction metrics are mandatory,
  and population-cap changes are versioned experimental interventions.
- **Recovery can erase the meaning of extinction.** It is canonical-only, zero-population-only,
  audited and reported separately; experimental clones remain extinct.
