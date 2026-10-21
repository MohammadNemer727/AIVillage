# Prior-art audit (Stage 0)

Audit date: 2026-10-20  
Workspace laptop: Intel Meteor Lake iGPU (Mesa Intel Graphics MTL); **no NVIDIA GPU**; `nvidia-smi` unavailable.  
Project policy: `deny.toml` allows only Apache-2.0, BSD-2/3-Clause, ISC, MIT, Unicode-3.0, Zlib for crate dependencies. Prefer ideas and clean-room reimplementation over vendoring foreign ALife source unless the license is already on that allow-list **and** an explicit project-license decision is recorded.

This document compares artificial-life projects relevant to Embodied AI Village V2. Companion file: [`THIRD_PARTY.md`](../THIRD_PARTY.md).

## Summary matrix

| Project | Language | License | Hardware | Determinism | Code reuse | Idea reuse |
| --- | --- | --- | --- | --- | --- | --- |
| [Polyworld](https://github.com/polyworld/polyworld) | C++ / Python | **APSL-2.0** | CPU + OpenGL/Qt | Not designed for bit-identical scientific replay | **No** (APSL copyleft-ish external-deploy rules; not in deny allow-list) | **Yes** — retina, energy-priced NN, Hebbian, evolvable traits |
| [ALIEN](https://github.com/chrxh/alien) | C++ / CUDA (HIP/AMD path exists) | **BSD-3-Clause** | **NVIDIA CUDA ≥ ~7.5** or AMD RDNA2+; **not Intel iGPU** | GPU sims; not a match for our fixed-point CPU determinism | **Legal yes, practical no** on this laptop; algorithms only if ported clean-room / attributed | **Yes** — modular organs, damage, energy, construction, dashboards |
| [The Bibites](https://thebibites.itch.io/the-bibites) | Closed (Unity/.NET binary) | **Proprietary EULA** (Omnia Studios) | Typical desktop GPU/CPU | N/A (game) | **No** — no reverse engineering / derivatives | **Yes** — digestion, organs, pheromones, lineage UI, procedural look |
| [Mutable](https://github.com/Ashwagandhae/mutable) | Rust (nannou) | **No license file** (`license: null` on GitHub) | CPU | Uses `rand` / floats; not our determinism model | **No** until author grants a license | **Design only** — body-tree nodes, blurred plant/animal |
| [biosim4](https://github.com/davidrmiller/biosim4) | C++ | **MIT** (LICENSE file; GitHub shows “Other”) | CPU + OpenMP; OpenCV/CImg for movies | Seeded configs; OpenMP can affect bit-identity if not careful | **Yes** (MIT) — prefer reading algorithms, not wholesale port | **Yes** — compact genome→NN, sensors/actions, pheromones, selection scenarios |
| [JaxLife](https://github.com/luchris429/JaxLife) | Python / JAX | **No license** declared | GPU-friendly JAX stack | JAX can be seeded; not our authoritative path | **No** without a license grant | **Yes** — agriculture / tools / culture experiment *designs* |
| [Cephalopods](https://github.com/jobtalle/cephalopods) | JavaScript | **MIT** | Browser CPU | Soft-body JS; not bit-identical research core | **Yes** (MIT) for reference ports with notice | Body morphology / soft segments |
| [The Life Engine](https://github.com/MaxRobinsonTheGreat/LifeEngine) | JavaScript | **GPL-3.0** | Browser | Cellular automaton | **No** as dependency or copied source (GPL; not in deny allow-list) | High-level cell-role ideas only |
| [Gene Pool / Swimbots](https://www.swimbots.com/code.html) | JavaScript | **MIT + Commons Clause** (non-commercial) | Browser | Physics-based swimming | **Avoid** — Commons Clause conflicts with open commercial reuse | Morphology + emergent locomotion ideas |

## Local hardware note (ALIEN)

ALIEN’s simulation kernel is GPU-native (CUDA; recent trees also advertise AMD HIP/SCALE). Upstream states NVIDIA compute capability on the order of RTX 20-series / 7.5+, or AMD RDNA2+.

**This development laptop has only Intel Iris/Meteor Lake integrated graphics.** ALIEN cannot run here without a discrete NVIDIA or supported AMD GPU (or a rented cloud GPU). Do **not** treat ALIEN as a runnable dependency or benchmark host for day-to-day V2 work. Study published descriptions, videos, and BSD-licensed algorithm *concepts*; implement CPU/fixed-point equivalents in Rust.

## Project-by-project

### Polyworld (Larry Yaeger et al.)

| Field | Detail |
| --- | --- |
| Repo | https://github.com/polyworld/polyworld |
| Language | C++, Python; Qt + OpenGL visualization |
| License | Apple Public Source License 2.0 (`LICENSE.txt`) |
| Hardware | Desktop CPU; graphics for retinal sensing / UI |
| Determinism | Classic ALife research platform; floating-point graphics-coupled sensing; not aimed at our golden-hash replay model |

**What it offers conceptually**

- Retinal / vision-based sensing in a continuous 2D world (agents must *see*).
- Genome-derived neural nets with **Hebbian** lifetime learning.
- Evolvable morphological and neural traits (size, speed, color, mutation rate, wiring).
- Energy metabolism tied to behavior and neural cost (energy-priced activity is a core research motif in Polyworld literature).

**Legal reuse**

- **Code:** Do **not** copy or vendor Polyworld source. APSL-2.0 imposes notice, license inclusion, and **source disclosure for externally deployed modifications**. It is **not** on this repo’s `deny.toml` allow-list.
- **Ideas:** Freely reuse high-level algorithms and experimental designs via clean-room Rust implementation (retina rays, metabolic neural cost, Hebbian / eligibility-style plasticity, evolvable sensor budgets).

**Recommended reuse for Rust V2**

- Borrow: raw vision without semantic labels; metabolic cost of sensing/thinking; genome→network developmental parameters; lifetime Hebbian / three-factor plasticity *concepts*.
- Skip: Qt/OpenGL stack, APSL source trees, graphics-coupled floating-point authority.

---

### ALIEN (chrxh)

| Field | Detail |
| --- | --- |
| Repo | https://github.com/chrxh/alien |
| Language | C++ host + CUDA kernels (HIP path for AMD) |
| License | BSD 3-Clause (Copyright Christian Heinemann) |
| Hardware | NVIDIA or AMD discrete GPU — **incompatible with this Intel-iGPU laptop** |
| Determinism | Massive particle GPU sim; not aligned with CPU fixed-point bit-identity |

**What it offers conceptually**

- Particle soft/rigid bodies, fluids, damage, adhesion, heat.
- Modular body “equipment”: sensors, muscles, weapons, constructors.
- Genomes / blueprints for offspring; ecosystem-scale dashboards.
- Strong energy / matter conservation aesthetics.

**Legal reuse**

- **Code:** BSD-3 allows reuse with copyright notice and disclaimer. Still a poor fit: CUDA-centric and wrong architecture for our CPU determinism goals. If any algorithm is ported, retain BSD attribution in `THIRD_PARTY.md`.
- **Ideas:** Modular organs, damage localization, construction, energy dashboards — implement as fixed-point Rust modules.

**Recommended reuse for Rust V2**

- Borrow: organ modularity, injury/damage, construction/ecosystem UI metrics, conservation bookkeeping *design*.
- Skip: running ALIEN locally; linking CUDA; copying particle GPU kernels.

---

### The Bibites (Omnia Studios)

| Field | Detail |
| --- | --- |
| Distribution | Steam / itch (closed source) |
| Language | Unpublished game binary (.NET/Unity ecosystem) |
| License | Proprietary EULA — no copy, reverse engineer, decompile, or derivative works |
| Hardware | Consumer PC |
| Determinism | Entertainment product; not a scientific replay engine |

**Legal reuse**

- **Code:** Off-limits entirely (including decompilation).
- **Ideas:** Digestion delay, organ allocation, pheromone fields, lineage visualization, procedural animal appearance — describe and reimplement independently.

**Recommended reuse for Rust V2**

- Borrow: UX and ecology *design* references only.
- Skip: any binary, asset, or decompiled logic.

---

### Mutable (Ashwagandhae)

| Field | Detail |
| --- | --- |
| Repo | https://github.com/Ashwagandhae/mutable |
| Language | Rust; nannou renderer; rayon; `rand` |
| License | **None published** (no `LICENSE`; GitHub `license: null`) |
| Hardware | CPU (local `cargo run --release`) |
| Determinism | Not fixed-point / counter-RNG; parallel rayon paths |

**What it offers conceptually**

- Organisms as **trees of functional nodes** (bones, muscles, sensors, brain).
- Unified plant / animal evolution (blurred “food circles” vs producers).
- Inspired by Bibites, Gene Pool, Life Engine, Cephalopods.

**Legal reuse**

- **Code:** Treat as **all rights reserved** until a permissive license is added or the author grants written permission. Do not copy source.
- **Ideas:** Body-tree morphology and plant/animal continuum — reimplement clean-room if useful.

**Recommended reuse for Rust V2**

- Borrow: body-tree *design* inspiration after license confirmation; compare API shapes conceptually only.
- Skip: cloning Mutable code into the workspace.

---

### biosim4 (David R. Miller)

| Field | Detail |
| --- | --- |
| Repo | https://github.com/davidrmiller/biosim4 |
| Language | C++ (OpenMP parallel steps; OpenCV/CImg for movies) |
| License | **MIT** (`LICENSE` in repo) |
| Hardware | CPU; Ubuntu/Debian-class build with OpenCV for video |
| Determinism | Config-driven; OpenMP parallel regions need care for bit-identical runs |

**What it offers conceptually**

- Compact genome encoding neural connections; small internal neuron caps.
- Grid world with sensors, action neurons, pheromones, barriers, selection scenarios.
- Excellent walkthrough documentation for genetics→brain mapping.
- Maintenance-only upstream; still valuable as a readable reference.

**Legal reuse**

- **Code:** MIT — may copy with copyright notice. Prefer extracting *algorithms* into idiomatic Rust rather than a line-for-line port (different world model: grid vs continuous fixed-point bodies).
- **Ideas:** Genome→sparse NN, pheromone layers, selection/experiment logging patterns.

**Recommended reuse for Rust V2**

- Borrow: genome wiring representation, sensor/action inventories as *inspiration*, pheromone diffusion ideas, experiment logging style.
- Skip: OpenCV movie path; grid-only authority; OpenMP non-determinism patterns.

---

### JaxLife (Lu, Beukman, Matthews, Foerster)

| Field | Detail |
| --- | --- |
| Repo | https://github.com/luchris429/JaxLife |
| Paper | arXiv:2409.00853 |
| Language | Python, JAX, Flax; pygame for viz |
| License | **No license** on GitHub (community checklist: “Choosing a license” unchecked) |
| Hardware | JAX prefers accelerator; CPU possible but heavy |
| Determinism | Seedable JAX; not our authoritative sim |

**What it offers conceptually**

- Embodied agents with RNNs + programmable robots (Turing-complete tooling).
- Reported emergent agriculture, terraforming, rudimentary communication, tool use.
- Compute-scaling discussion for open-ended culture / technology.

**Legal reuse**

- **Code:** Do **not** copy or depend on JaxLife source without an explicit license.
- **Ideas / paper:** Experiment designs (social transmission, agriculture, tool-use measures, cultural accumulation controls) are fair game as research methodology, cited appropriately.

**Recommended reuse for Rust V2**

- Borrow: Stage 8–9 experiment designs (traditions, cumulative culture controls, demonstrator replacement).
- Skip: JAX/Flax stack; unlicensed code; GPU-first agent DNNs as the V1/V2 authority.

---

### Other relevant ALife (secondary)

| Project | License | Reuse stance |
| --- | --- | --- |
| Cephalopods | MIT | OK to study / port morphology ideas with notice |
| Life Engine | GPL-3.0 | Ideas only; no code in our tree |
| Gene Pool / Swimbots | MIT + Commons Clause | Non-commercial clause — avoid code; ideas OK |

## Research since 2024: development, contingency and open-ended agents

Audit date: 2026-10-21. This section covers research published in 2024 or later that bears on the
founder's reward-free affordance learner, the infant mobile-paradigm test in
[`contingency-test.md`](contingency-test.md), and the planned habituation, object-permanence and
means-end tests. Each entry was checked against its arXiv abstract, journal or proceedings page;
where only an indexing record (for example Europe PMC) was reachable, the entry was checked
against that. All entries are idea sources only: no code is reused. "Preprint" means no
peer-reviewed venue was listed when checked.

### Where AI Village sits

- **Closest prior work:** Xu & Triesch (2025, 2026) score actions by how far the outcome
  distribution after an action differs from the baseline distribution and test it on the infant
  mobile paradigm. The founder uses the same idea with each outcome's average and spread instead
  of its full distribution. It passes their standard, inverted and bimodal paradigms and a
  seen-only paradigm; see [`contingency-test.md`](contingency-test.md) for current results.
- **Not found in the 2024+ literature:**
  - a reward-free, label-free agent in a Luanti/Minetest world; the published Luanti work
    (Craftium, ARM-FM) is reward-driven;
  - a mobile-paradigm model that reports a yoked (non-contingent) control together with
    bit-exact, seeded replay;
  - habituation or object-permanence tests run on a curiosity-driven embodied agent rather than on
    a video model.
- **Design lessons repeated across papers:**
  - Track outcome spread or full distributions, not only averages: CAIS, Curiosity-Critic and
    epistemic free-energy estimators.
  - Reward learning progress, not raw surprise, to resist noise: Learning Progress Monitoring and
    MAGELLAN.
  - Report more than a rate increase: per-run trajectories (Fujihira et al.), anticipation
    (Rosales et al.), flexible redirection (Bednarski et al.) and varied movement strategies
    (Spisak et al., *Science Robotics*).
  - Memory span and motor noise matter: Spisak et al., ICDL 2025 and IEEE TCDS 2026.

### What AI Village has adopted

As of 2026-10-21, the founder and its test harness implement these lessons. Results are on
deterministic synthetic fixtures, and the core contingency result is confirmed in real Luanti: with
a pushable cube the founder's hand pulses rise from 6.8 to 15.4 per 100 decisions and return to
8.1 in extinction, while an inert cube leaves them flat (see
[`luanti-learning-run.md`](luanti-learning-run.md)).

| Lesson | Source | Implementation | Result |
| --- | --- | --- | --- |
| Mobile paradigm with baseline, connected and extinction phases | Rovee-Collier; Xu & Triesch 2025, 2026; Spisak et al. 2026 | `sim-luanti contingency` | The founder passes on 7 of 8 seeds |
| Yoked (non-contingent) control | Venditti et al. 2025 | Each seed replays another seed's consequences | Yoked runs stay near baseline |
| Inverted and bimodal paradigms | Xu & Triesch 2026 | Pulse removes contact; pulse changes only contact spread | Both pass |
| Judge outcome spread as well as average | CAIS; Curiosity-Critic; Yiu et al. | Per-experiment and overall mean absolute deviation | Bimodal went from 0/8 to passing seeds |
| Judge each sense against its own usual variation | Spisak et al. 2026 (strategy spectrum) | Differences scaled by each outcome's overall spread | A seen-only consequence can compete with posture changes |
| Vision of objects, not only walls | Infant mobile studies are visual | Luanti eye rays see physical entities; learner tracks largest ray motion and change | Seen-only paradigm passes |
| Memory that fades | Spisak et al., ICDL 2025 | Unused experiment memories drift to normal over about 32 decisions | Extinction ends instead of dragging on |
| Delayed consequences | Millar 1972; MIMo v2 sensorimotor delays | Eligibility trace over the last 4 experiments | Learning fades with delay, as in infants |
| Object in the default view | Every mobile study and model (Xu & Triesch; Spisak et al.; Rosales et al.) | In Luanti learning runs the cube hangs at the founder's eye height and swings back after a push | Real-game hit rate rose from 31% to 56% of pushes |
| Situations the founder discovers itself | Oudeyer's IAC and SGIM region splitting (via Nguyen 2024); VACERL step selection; Curiosity→Competence on hand-picked vs learned state | `ContextMode::Learned`: starts with one situation and splits on one of 21 generic sense facts when some experiment's effect clearly differs between the two sides; off by default | Synthetic reaching: every seed discovers "near ahead", late hit rate 20.8% vs 12.6% without situations. Real game: 25.1%, better than hand-picked situations (20.6%) but below none (29.5%): its first splits are on its own movement, which matter for walking but fragment the experience of pushing. Next: per-experiment situations |
| Full outcome distributions | Xu & Triesch CAIS (code: github.com/trieschlab/cais); expectiles from distributional RL (Rowland et al.) | `OutcomeModel::Expectiles`: nine expectiles per feature, compared by mean absolute gap; off by default | Not better here: passes standard and inverted on 6/8 seeds, bimodal 3/8 and seen 2/8, against 8, 8, 8 and 6 for average and spread; reaching late hit rate 18.3% vs 12.6%. A first version with stochastic quantiles learned too slowly on rare outcomes (1/8) |
| Context-conditioned contingency, with per-situation curiosity and shrinkage to pooled statistics | Seitzer et al. causal action influence (via Mantiuk et al. 2025); IAC count-based curiosity | `ContextMode::SenseBits`: four fixed sense bits, off by default | Synthetic reaching test: late hit rate 26.4% vs 12.6% context-free and 13.5% random. Real game: worse (20.6% vs 29.5%), because "close ahead" fires on walls and floor as often as on the cube and self-motion splits memory |
| Bouts of repeated movement | Infant kicking in bouts (Rovee-Collier; Fujihira et al. 2025) | Each chosen experiment is held for 3 decisions | Delays of up to 2 decisions are learned on 8/8 seeds; in Luanti 31% of pushes reach the cube, up from 18% |
| Report curves, strategies and anticipation, not one rate | Fujihira et al. 2025; Rosales et al. 2025; Spisak et al. 2026 | Protocol harness | Per-seed curves, strategy mix and anticipation in every report |
| Do not judge the extinction burst as failure | Xu & Triesch 2026; Diprossimo et al. 2026 | Verdict uses late extinction only | Burst is reported, not penalized |

Learning-progress curiosity (Hou et al.; MAGELLAN) and CAIS-style full outcome distributions
are available as alternatives and compared in every report; contingency judged by average and
spread remains the default. Fixed sense bits confound in the real game, as the research warned
for hand-picked state features. Situations the founder learns itself do better than hand-picked
ones in the game but not yet better than none, because one shared set of situations serves every
experiment; the next step is situations per experiment. Not yet adopted: CAIS's surprise
term, flexible-control and anticipation-timing tests (Bednarski et al.; Rosales et al.), and the
planned habituation, object-permanence and means-end protocols.

### Infant contingency learning and the mobile paradigm

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Xu & Triesch, [*Infants and Mobiles: Developing an Understanding of Cause and Effect*](https://doi.org/10.1111/desc.70211) (Developmental Science 29(4), 2026) | An active-learning causal model discovers the mobile contingency without being told cause or effect. An added expectation-violation mechanism produces the extinction burst. Simple reinforcement fails when the action stops the mobile (inverted); an entropy-based controllability model fails when the action only changes variability (bimodal). | Source of our inverted and bimodal paradigms. An average-only founder passed inverted and failed bimodal, as their analysis predicts; tracking spread fixed bimodal. |
| Xu & Triesch, [*From Kicking to Causality: Simulating Infant Agency Detection with a Robust Intrinsic Reward*](https://arxiv.org/abs/2507.15106) (preprint, 2025) | The Causal Action Influence Score (CAIS) is the 1-Wasserstein distance between the outcome distribution after an action and the baseline outcome distribution, learned by quantile regression. It is tested on a MIMo infant in MuJoCo, with baseline, attached and extinction phases. Only CAIS survives random forces on the mobile, and CAIS plus surprise gives an extinction burst. No yoked control or seed count is reported. | Closest prior work. The founder now tracks average and spread, which fixed the bimodal failure; full CAIS-style distributions remain a possible upgrade. |
| Spisak, Tcaci Popescu, Wermter et al., [*A computational model of infant sensorimotor exploration in the mobile paradigm*](https://arxiv.org/abs/2504.17939) (IEEE TCDS 18(3), 2026) | Combines action-outcome prediction, exploration, motor noise, a preferred activity level and bio-inspired motor control. It reproduces preference for the connected limb, a burst after disconnection, and data from gradual and all-or-none connection studies. Ablations show prediction, exploration, motor noise and motor control are each needed. | Independent model of the same paradigm, with code at ctu-vras/mobile-paradigm-model. Its finding that motor noise is needed is a contrast with our deterministic founder. |
| Spisak, Tcaci Popescu, Rustler et al., [*Robot in a crib: How a playing robot helps us understand sensorimotor contingency learning*](https://doi.org/10.1126/scirobotics.aed4106) (Science Robotics 11(117), 2026) | An iCub humanoid driven by prediction and curiosity plays with a physical mobile across conditions. Learning appears as a range of exploratory strategies from a prediction-versus-curiosity trade-off, not as any single measure such as total movement. | Warns that our 1.5×-rate criterion can miss learning. We should log per-action strategy measures as well. |
| Spisak, Benad, Heidersberger et al., [*Computational models of the emergence of self-exploration in 2-month-old infants*](https://tore.tuhh.de/entities/publication/5da8e6a5-1bb7-4c54-937c-c1a8c1d4057e/) (IEEE ICDL 2025) | Models why 2-month-olds, unlike newborns, respond to mouth-contingent sounds. Seven candidate factors were tested in supervised and RL models; growing memory capacity is the key factor. | Suggests the founder's memory span (currently an 8-visit horizon) controls whether contingency detection appears at all. |
| Cuevas & Colombo, [*A Quarter Century of Research on Infant Contingency Learning*](https://doi.org/10.1016/j.infbeh.2025.102068) (Infant Behavior and Development 80, 2025) | Reviews operant and mobile research from 2000 to 2025: agency, goal blockage, individual differences, motion capture, eye tracking and modelling. Whether infants show agency is still open, and standard learning metrics miss nonmonotonic change. | Current reference for which metrics and criteria to report. |
| Fujihira, Watanabe & Taga, [*Individual differences in how infants change behaviours from spontaneous to instrumental*](https://doi.org/10.1038/s44271-025-00333-3) (Communications Psychology 3, 2025) | An arm-tethered mobile study with 185 infants aged 2–3 months, analysed with time-series clustering and dynamical modelling. Smooth group curves hide varied individual paths, and the variation comes from spontaneous movement before learning. | Report per-seed trajectories, not only pooled rates. The founder's baseline action statistics should shape its learning curve. |
| Bednarski, Rothmaler, Hofmann & Grosse Wiesmann, [*Infants in Control*](https://doi.org/10.1111/cdev.70022) (Child Development 96(6), 2025) | 45 infants aged about 8.5 months uncovered hidden images with their gaze, across baseline, contingent and disruption phases. They directed gaze flexibly to the most informative areas, and this strengthened during the contingent phase. The authors argue agency needs flexible adaptation, not just more responding. | Motivates a "flexible control" test. The founder's look and turn actions are a natural gaze-contingent analogue. |
| Venditti, Elkin, Williams et al., [*Contingency enables the formation of social expectations about an artificial agent*](https://doi.org/10.1111/infa.12614) (Infancy 30(1), 2025) | 7–8-month-olds' babbling triggered a remote-controlled car or an adult, either contingently or on a yoked schedule. Infants vocalised more to contingent partners and more when the partner paused. | The same contingent-versus-yoked contrast as our yoked control, with a non-social effector. |
| Diprossimo, Meyer, Lüdemann et al., [*Infant Vocal Behavior During Contingent Vocal Imitation and Its Interruption*](https://doi.org/10.1111/infa.70080) (Infancy 31(2), 2026) | An artificial agent imitated 5–6-month-olds' vocalisations during connect and disconnect phases. Vocalising rose linearly during connect; the burst-like trend at disconnect was not significant. | Connect and extinction in a vocal channel. The weak burst cautions against treating the extinction burst as robust. |
| Rosales, Pulido, Winstein et al., [*Infants Display Anticipatory Gaze During a Motor Contingency Paradigm*](https://doi.org/10.3390/s25030844) (Sensors 25(3), 2025) | Leg movement in 15 infants aged 6–9 months triggered a clapping robot. Learners by the 1.5× criterion looked predictively, about 0.31 s before activation, but did not differ from non-learners on gaze. The authors question the 1.5× criterion. | Anticipation is an alternative learning signature that the founder could show through its forward predictions. |
| Rosales, Pulido, Sideris et al., [*Behavioral differences between infants at and not at elevated risk for autism during a contingency paradigm*](https://doi.org/10.1371/journal.pone.0340732) (PLoS ONE 21(1), 2026) | The same leg-to-robot paradigm with 30 infants. No clear group differences, though some higher-likelihood infants showed different visuomotor patterns. | Another dataset for robot-mobile variants. |
| Karasik, Robinson & Dodojonova, [*Effects of Gahvora Cradling on Learning, Motor Behavior, and Expression of Affect in 3-Month-Olds in Tajikistan*](https://doi.org/10.3390/brainsci16070736) (Brain Sciences 16(7), 2026) | A home mobile task with 26 infants who spend most of the day bound in a cradle. All learned, kicking stayed above baseline during extinction, and the learning rate depended on movement opportunity before the test. | Prior activity changes learning, so we should control what the founder does before the baseline phase. |
| Khodadadzadeh, Sloan, Jones et al., [*Artificial intelligence detects awareness of functional relation with the environment in 3 month old babies*](https://doi.org/10.1038/s41598-024-66312-6) (Scientific Reports 14, 2024) | Classifiers on motion-capture joint data identify mobile-paradigm stages. The foot coupled to the mobile changes most across stages. | Per-effector statistics should be stage signatures: the hand pulse should change most across our phases. |
| Chinn, Noonan, Patton & Lockman, [*Tactile localization promotes infant self-recognition in the mirror-mark test*](https://doi.org/10.1016/j.cub.2024.02.028) (Current Biology 34(6), 2024) | Infants who practised reaching to touches on their own body passed mirror self-recognition earlier than controls. | Learning action-to-body mappings supports later self-knowledge, the path the founder's body contract follows. |
| Stupperich, Dathe, DiMercurio et al., [*Quantity of spontaneous touches to body and surface in very preterm and healthy term infants*](https://doi.org/10.3389/fpsyg.2024.1459009) (Frontiers in Psychology 15, 2024) | Video-coded spontaneous touches: very preterm infants touched less (8.87 per minute) than term infants (13.19 per minute). The study is exploratory, with a small term group. | Reference rates for spontaneous hand activity before any contingency. |

### Simulated infants, body discovery and self-models

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| López, Lenz, Fedozzi et al., [*MIMo grows! Simulating body and sensory development in a multimodal infant model*](https://arxiv.org/abs/2509.09805) (IEEE ICDL 2025) | MIMo v2 adds a body that grows from birth to 24 months, foveated vision with developing acuity, sensorimotor delays, inverse kinematics and a random environment generator. It is the body used by BabyBench and by the Triesch group. | A possible external comparison body. Its sensorimotor delays suggest testing contingency with a delayed hand-pulse outcome. |
| Zarifis, Chalkiadakis, Chardouveli et al., [*Baby Sophia: A Developmental Approach to Self-Exploration through Self-Touch and Hand Regard*](https://arxiv.org/abs/2511.09727) (preprint, 2025) | Curiosity-only RL in BabyBench (MIMo body). Self-touch reached 0.85 coverage (32 of 34 body parts); hand regard was weaker, at about 50.6% visibility. | The nearest benchmark task to "which action causes which bodily outcome". |
| Philipp, López & Triesch, [*Embodiment Shapes Rolling Behavior in a Multimodal Infant Model*](https://arxiv.org/abs/2606.17456) (preprint, accepted IEEE ICDL 2026) | MIMo learns to roll from back to front with RL, using proprioception and vestibular sensing. Age-appropriate body shape speeds learning. | Body parameters change which outcomes can be discovered, relevant to the founder's body contract. |
| Kim, Kanazawa, Yoshida & Kuniyoshi, [*Emergence of Goal-Directed Behaviors via Active Inference with Self-Prior*](https://arxiv.org/abs/2504.11075) (Developmental Science 29(6), 2026) | A density model of the agent's own sensations (a "self-prior") inside active inference. A simulated infant reaches toward a touch without any reward, and the self-prior acts as a learned body schema. | A different reward-free route to body discovery: deviation from one's own sensory history. |
| Kim, Kanazawa & Kuniyoshi, [*Active Inference with a Self-Prior in the Mirror-Mark Task*](https://arxiv.org/abs/2604.09673) (preprint, accepted IEEE ICDL 2026) | A simulated infant using vision and proprioception finds and removes a face mark seen in a mirror in about 70% of trials. | Shows contingency plus a self-model reaching self-recognition, a possible later founder test. |

### Contingency, causal influence and empowerment

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Bhaskara & Wang, [*Curiosity-Critic: Cumulative Prediction Error Improvement as a Tractable Intrinsic Reward for World Model Training*](https://arxiv.org/abs/2604.18701) (ICML 2026 EIML workshop) | Reward is prediction error minus a learned per-transition "noise floor". It goes to zero for random transitions and stays high for learnable ones, and trains world models faster than prediction-error, count and RND baselines. | A per-action noise floor is a cheap, deterministic way to account for outcome spread in the founder's score. |
| Furutanpey & Dustdar, [*Principled Direction-Free Intrinsic Motivation through Model-Free Epistemic Free-Energy Estimators*](https://arxiv.org/abs/2607.16858) (IWAI 2026) | Combines information gain (via pseudocounts) with a penalty on irreducible variance and a short-horizon gate, with stationarity and concentration guarantees. | Models outcome variance as its own term, as the founder now does with per-experiment spread. |
| Mantiuk, Zhou & Wu, [*From Curiosity to Competence: How World Models Interact with the Dynamics of Exploration*](https://arxiv.org/abs/2507.08210) (CogSci 2025) | Compares curiosity (novelty, information gain) with competence (empowerment) in tabular and Dreamer agents. They steer exploration differently, and combining them helps; control matters more in noisy worlds. | Supports keeping the founder's contingency score and curiosity bonus as separate drives. |
| Yiu, Allen, Ginosar & Gopnik, [*Empowerment Gain and Causal Model Construction*](https://arxiv.org/abs/2512.08230) (Philosophical Transactions A, accepted 2026) | Links causal Bayes nets and empowerment. Children and adults use both controllability and variability cues to infer causes and design interventions. | Human sensitivity to variability motivated the founder's spread tracking, which made it pass the bimodal paradigm. |
| Cao, Feng, Fang et al., [*Towards Empowerment Gain through Causal Structure Learning in Model-Based RL*](https://arxiv.org/abs/2502.10077) (ICLR 2025) | Learns a causal dynamics model and explores by maximising empowerment under it, refining the model with the new data. Tested across six environments, including pixel tasks. | Shows how a per-action causal-influence model becomes an exploration policy. |
| Tiomkin, Salge & Polani, [*Process empowerment for robust intrinsic motivation*](https://doi.org/10.1088/2632-072x/adf2ec) (Journal of Physics: Complexity 6(3), 2025) | A feedback-aware "process empowerment" that is more robust than open-loop empowerment in noisy and adversarial tasks. | A reward-free controllability measure designed for stochastic outcomes. |
| Nguyen, Le & Venkatesh, [*Variable-Agnostic Causal Exploration for Reinforcement Learning*](https://arxiv.org/abs/2407.12437) (preprint, 2024) | Attention picks out key observation-action steps without predefined causal variables, and a causal graph over them gives intrinsic rewards or subgoals. It helps in grid, 2D game and robot tasks, including Noisy-TV settings. | Causal exploration without hand-named variables, as the founder's no-label contract requires. |
| Aoun Durand & Khamassi, [*Incentivizing Exploration with Causal Curiosity as Intrinsic Motivation*](https://openreview.net/forum?id=LZI8EFLoFD) (NeurIPS 2024 IMOL workshop) | Rewards change in the agent's own causal model, with separate values for extrinsic and discovery rewards. | An alternative to a fixed contingency score: reward updates to the causal model itself. |
| Schneider, Krug, Vaskevicius et al., [*Information-Theoretic Policy Pre-Training with Empowerment*](https://arxiv.org/abs/2510.05996) (preprint, 2025) | "Discounted empowerment" balances short- and long-horizon control. Pre-training to maximise it makes later task learning more data-efficient. | Supports reward-free controllability-driven pre-training like the founder's. |
| Mazzaglia, Cohen & Dijkman, [*Information-driven Affordance Discovery for Efficient Robotic Manipulation*](https://arxiv.org/abs/2405.03865) (preprint, 2024) | An information term directs interaction toward discovering affordances: grasping, stacking and drawers in simulation, and grasping on a real xArm 6. | Affordance-level analogue of the founder's action-outcome discovery. |
| Hamon, Etcheverry, Chan et al., [*Discovering Sensorimotor Agency in Cellular Automata using Diversity Search*](https://arxiv.org/abs/2402.10236) (Science Advances 11(44), 2025) | Diversity search, curricula and gradient descent find self-maintaining "individuals" in cellular automata that move and react to obstacles. They generalise to scale changes and perturbations. | Treats agency as something to discover without reward; a conceptual neighbour. |

### Noise robustness and learning progress

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Hou, An & Du, [*Beyond Noisy-TVs: Noise-Robust Exploration Via Learning Progress Monitoring*](https://arxiv.org/abs/2509.25438) (ICLR 2026) | Rewards improvement of the dynamics model, using an error model of the previous iteration. Unlearnable noise earns nothing. Best state coverage in noisy MiniWorld mazes and robust on noisy Atari. | A principled replacement for the founder's recency curiosity. |
| Lidayan, Dennis & Russell, [*BAMDP Shaping: a Unified Framework for Intrinsic Motivation and Reward Shaping*](https://arxiv.org/abs/2409.05358) (ICLR 2025) | Treats intrinsic rewards as shaping in Bayes-adaptive MDPs, splitting value into information value and physical value. Explains when bonuses help or get hacked, and shows potential-based shaping resists hacking. | Theory for checking whether the founder's bonuses can be gamed. |
| Oh, Kim & Kim, [*A Temporally Correlated Latent Exploration for Reinforcement Learning*](https://arxiv.org/abs/2412.04775) (preprint, 2024) | An action-conditioned latent distribution avoids over-rewarding unpredictable states. Robust to Noisy-TV in MiniGrid and stochastic Atari. | Scores actions by more than their mean outcome. |
| Jiang, Liu, Yang et al., [*Episodic Novelty Through Temporal Distance*](https://arxiv.org/abs/2501.15418) (ICLR 2025) | Episodic novelty from a contrastively learned temporal distance. Beats prior methods on MiniGrid, Crafter and MiniWorld. | A stronger alternative to recency-based curiosity. |
| Zheng, Henaff, Zhang et al., [*Online Intrinsic Rewards for Decision Making Agents from Large Language Model Feedback*](https://arxiv.org/abs/2410.23022) (RLC 2025) | An LLM labels experience asynchronously, and the labels are distilled into an intrinsic reward online. State of the art on hard NetHack tasks. | A semantic prior the founder deliberately avoids; it would also break determinism unless cached. |
| Sancaktar, Gumbsch, Zadaianchuk et al., [*SENSEI: Semantic Exploration Guided by Foundation Models to Learn Versatile World Models*](https://arxiv.org/abs/2503.01584) (ICML 2025) | Distils a VLM "interestingness" signal into a world model and explores with it plus uncertainty, from pixels and low-level actions. The authors argue pure information gain finds only low-level interactions. | A caution about how far a pure contingency signal scales. |
| Andres & Del Ser, [*Fostering Intrinsic Motivation in Reinforcement Learning with Pretrained Foundation Models*](https://arxiv.org/abs/2410.07404) (NeurIPS 2024 IMOL workshop) | CLIP embeddings drive intrinsic rewards in MiniGrid; representation quality matters as much as the bonus. | The founder's outcome features are its representation, and they limit what it can discover. |
| Yuan, Creus Castanyer, Li et al., [*RLeXplore: Accelerating Research in Intrinsically-Motivated Reinforcement Learning*](https://arxiv.org/abs/2405.19548) (TMLR 2025) | A modular framework with eight intrinsic rewards (RND, E3B, ICM, NGU and others) and a study of which implementation details matter. | Reference baselines for comparing the founder's learner. |
| Villalobos-Arias, Forbes, Wang et al., [*Minding Motivation: The Effect of Intrinsic Motivation on Agent Behaviors*](https://arxiv.org/abs/2507.19725) (preprint, 2025) | Intrinsic bonuses in MiniGrid change behaviour (reward hacking), and Generalized Reward Matching reduces it in some cases. | Check behaviour, not only coverage, when tuning the founder. |
| Srivastava & Singh, [*Autotelic Reinforcement Learning: Exploring Intrinsic Motivations for Skill Acquisition in Open-Ended Environments*](https://arxiv.org/abs/2502.04418) (Int. J. Computer Trends and Technology 73, 2025) | A survey separating knowledge-based from competence-based intrinsic motivation, with proposed metrics. | Vocabulary: the founder is competence-based (contingency) plus knowledge-based (recency). |

### Developmental robotics and open-ended learning

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Romero, Baldassarre, Duro et al., [*A Motivational Architecture for Open-Ended Learning Challenges in Robots*](https://arxiv.org/abs/2506.18454) (RLDM 2025) | H-GRAIL combines several intrinsic motivations to discover goals, learn and chain skills, and adapt to changing environments, validated on a real robot. | The natural layer above the founder's primitive-level learner. |
| Gaven, Carta, Romac et al., [*MAGELLAN: Metacognitive predictions of learning progress guide autotelic LLM agents in large goal spaces*](https://arxiv.org/abs/2502.07709) (ICML 2025) | Agents predict their own competence and learning progress, generalising across related goals, and pick goals by learning progress. Only method to master a large, changing goal space. | Learning progress as the signal for which contingency the founder probes next. |
| Nguyen, [*The intrinsic motivation of reinforcement and imitation learning for sequential tasks*](https://arxiv.org/abs/2412.20573) (habilitation thesis, 2024) | One learning-progress motivation chooses the curriculum: explore or imitate, which actions, which tutor; studies symbols emerging from sensorimotor exploration. | Relevant when the founder moves from single actions to sequences. |
| Lintunen, Ady & Guckelsberger, [*Diversity Progress for Goal Selection in Discriminability-Motivated RL*](https://arxiv.org/abs/2411.01521) (NeurIPS 2024 IMOL workshop) | Chooses goals by improvement in how well they can be told apart, learning distinct skills faster without collapse. | A light progress-based selector for which primitive to test next. |
| Tinker, Doya & Tani, [*Curiosity-Driven Development of Action and Language in Robots Through Self-Exploration*](https://arxiv.org/abs/2510.05013) (preprint, 2025) | Simulated robots learn commands such as "push red cube" through curiosity-driven exploration. Memorisation precedes generalisation, with child-like U-shaped curves. | A developmental route from contingencies to grounded symbols. |
| Zhang, Ge, Yoo et al., [*Playful Agentic Robot Learning*](https://arxiv.org/abs/2606.19419) (preprint, 2026) | Agents "play" before tasks: propose tasks, write code policies, diagnose failures and keep successful skills. Play-learned skills improve LIBERO-PRO by 20.6 points and help other agents. | Supports a play phase before tasks, though its play is LLM-driven. |
| Zhao, Weber & Wermter, [*Agentic Skill Discovery*](https://arxiv.org/abs/2405.15019) (preprint, 2024) | An LLM proposes tasks and rewards, RL learns them, and a VLM checks reliability, growing a skill library from zero. | The LLM-driven version of autonomous discovery the founder avoids. |
| Li, Yu, Wu et al., [*Growing from Exploration: A self-exploring framework for robots based on foundation models*](https://arxiv.org/abs/2401.13462) (preprint, 2024) | An infant-inspired robot generates its own tasks and keeps useful skills, using foundation-model feedback, with a real-robot demonstration. | Contrast with the founder's non-LLM, deterministic approach. |
| Hughes, Dennis, Parker-Holder et al., [*Open-Endedness is Essential for Artificial Superhuman Intelligence*](https://arxiv.org/abs/2406.04268) (ICML 2024 position track) | Defines open-endedness as novelty plus learnability for an observer, and argues it is necessary for superhuman AI. | A vocabulary for judging whether the founder's discoveries are open-ended. |
| Faldor, Zhang, Cully et al., [*OMNI-EPIC: Open-endedness via Models of human Notions of Interestingness with Environments Programmed in Code*](https://arxiv.org/abs/2405.15568) (ICLR 2025) | Foundation models write new environments and rewards that are learnable and interesting, adapting to the agent's progress. | The opposite design (generated rewarded tasks); a contrast in related work. |

### Minecraft-like, Crafter and Luanti agents

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Malagón, Ceberio & Lozano, [*Craftium: Bridging Flexibility and Efficiency for Rich 3D Single- and Multi-Agent Environments*](https://arxiv.org/abs/2407.03969) (ICML 2025) | A Gymnasium and PettingZoo framework on the Luanti engine with Lua-defined environments, running over 2,000 more steps per second than comparable Minecraft frameworks. Reward- and episode-based. | Closest platform: Luanti plus Lua mods. Craftium builds rewards into the engine; we connect a reward-free Rust agent over a loopback bridge. |
| Creus Castanyer, Mohamed, Castro et al., [*ARM-FM: Automated Reward Machines via Foundation Models for Compositional Reinforcement Learning*](https://arxiv.org/abs/2510.14176) (ICLR 2026) | Foundation models generate reward machines from language. On a sparse Craftium diamond-mining task, PPO with them learns the full sequence. | One of the few published Luanti uses, and the opposite philosophy (LLM-shaped reward). |
| Lidayan, Du, Kosoy et al., [*Intrinsically-Motivated Humans and Agents in Open-World Exploration*](https://arxiv.org/abs/2503.23631) (preprint, 2025) | Adults, children and RL agents explore Crafter without goals. Only entropy and empowerment track human progress; empowerment keeps rising after entropy plateaus. | Best human baseline for control-based exploration in a voxel-like world. |
| Mohamed, Ji, Eysenbach et al., [*Temporal Representations for Exploration: Learning Complex Exploratory Behavior without Extrinsic Rewards*](https://arxiv.org/abs/2603.02008) (preprint, 2026) | C-TeC drives the agent toward states with unpredictable futures using temporal contrastive representations. With no reward, it beats RND, ICM and E3B in mazes, bin-picking and Craftax-Classic. | Current best reward-free exploration in the Crafter family. |
| Yu & Lu, [*ADAM: An Embodied Causal Agent in Open-World Environments*](https://arxiv.org/abs/2410.22194) (ICLR 2025) | A Minecraft agent builds a causal graph (a tech tree) from its own interaction and stays effective in modified Minecraft where prior knowledge fails. Uses multimodal-LLM perception. | Closest conceptual match in Minecraft, but it relies on labelled items and LLM perception, which the founder excludes. |
| Matthews, Beukman, Ellis et al., [*Craftax: A Lightning-Fast Benchmark for Open-Ended Reinforcement Learning*](https://arxiv.org/abs/2402.16801) (ICML 2024) | Crafter in JAX, up to 250× faster, plus a harder Craftax. Intrinsic exploration methods (ICM, E3B) make little progress on full Craftax. | Standard fast testbed; its negative intrinsic-motivation result is worth citing. |
| Tang, Li, Liang et al., [*Mars: Situated Inductive Reasoning in an Open-World Environment*](https://arxiv.org/abs/2410.08126) (NeurIPS 2024 Datasets and Benchmarks) | A Crafter variant with altered terrain, survival and dependency rules, so agents must induce rules from interaction. RL and LLM agents struggle. | Tests discovering world rules without priors, as the founder must. |
| Ju, Sun, Wu et al., [*MineExplorer: Evaluating Open-World Exploration of MLLM Agents in Minecraft*](https://arxiv.org/abs/2605.30931) (EMNLP 2026) | Multi-step tasks with hidden prerequisites and little Minecraft-specific knowledge. Strong MLLM agents degrade sharply when prerequisites must be coordinated. | Its "hidden prerequisites" framing helps design later means-end tests. |
| Carta, Romac, Gaven et al., [*HERAKLES: Hierarchical Skill Compilation for Open-ended LLM Agents*](https://arxiv.org/abs/2508.14751) (preprint, 2025) | An LLM picks subgoals and a low-level controller compiles successful behaviour into reusable skills, tested in Crafter. | Open-ended skill composition, but with language goals. |
| Liu, Li, Zhang et al., [*Odyssey: Empowering Minecraft Agents with Open-World Skills*](https://arxiv.org/abs/2407.15325) (preprint, 2024) | An LLM agent with 223 skills and a wiki-tuned LLaMA-3, plus a benchmark that includes autonomous exploration. | Exploration that depends on wiki knowledge, unlike the founder's. |
| Li, Xie, Shao et al., [*Optimus-1: Hybrid Multimodal Memory Empowered Agents Excel in Long-Horizon Tasks*](https://arxiv.org/abs/2408.03615) (NeurIPS 2024) | A Minecraft agent with a knowledge graph and an experience pool, near human level on many long tasks. | An LLM-era counterpart to the founder's learned contingency memory. |
| Cai, Wang, Lian et al., [*ROCKET-1: Mastering Open-World Interaction with Visual-Temporal Context Prompting*](https://arxiv.org/abs/2410.17856) (preprint, 2024) | A VLM planner drives a low-level policy through object segmentation masks, with a large gain in Minecraft interaction. | Shows the field's reliance on object segmentation and labels. |
| Cai, Mu, He et al., [*MineStudio: A Streamlined Package for Minecraft AI Agent Development*](https://arxiv.org/abs/2412.18293) (preprint, 2024) | An open toolkit covering simulator, data, models, training, inference and benchmarks. | Java Minecraft counterpart to our Rust and Luanti stack. |
| Burchi & Timofte, [*Accurate and Efficient World Modeling with Masked Latent Transformers*](https://arxiv.org/abs/2507.04075) (preprint, 2025) | EMERALD is the first method to beat human experts on Crafter within 10M steps. | The current reward-driven performance ceiling on Crafter. |
| Guo, Ye, He et al., [*MineWorld: a Real-Time and Open-Source Interactive World Model on Minecraft*](https://arxiv.org/abs/2504.08388) (technical report, 2025) | An autoregressive frame-and-action Transformer at 4–7 FPS, with metrics for how well frames follow actions. | Its action-controllability metrics are a learned-pixel analogue of contingency measurement. |
| Zhang, Peng, Wang et al., [*Matrix-Game: Interactive World Foundation Model*](https://arxiv.org/abs/2506.18701) (technical report, 2025) | A 17B Minecraft world model with the GameWorld Score benchmark, including controllability and physical-rule understanding. | Its physical-rule evaluation asks whether action consequences are learned at all. |
| Decart and Etched, [*Oasis: A Universe in a Transformer*](https://oasis-model.github.io/) (project release, 2024) | A real-time model that generates Minecraft-like play from keyboard input with no game engine. Reviewers noted it loses track of world layout. | A learned simulator instead of an engine; Luanti gives us a consistent ground-truth world instead. |

### Learning like a child: first-person data, intuitive physics and developmental benchmarks

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Garrido, Ballas, Assran et al., [*Intuitive physics understanding emerges from self-supervised pretraining on natural videos*](https://arxiv.org/abs/2502.11831) (preprint, 2025) | Violation-of-expectation tests show V-JEPA has object permanence and shape consistency, while pixel predictors and multimodal LLMs are near chance. Even a week of video gives above-chance results. | Template for the planned object-permanence test: read surprise from prediction error. |
| Bordes, Garrido, Kao et al., [*IntPhys 2: Benchmarking Intuitive Physics Understanding In Complex Synthetic Environments*](https://arxiv.org/abs/2506.09849) (preprint, 2025) | Possible-versus-impossible videos on permanence, immutability, continuity and solidity. Leading models are near chance; humans are near perfect. | Event-pair designs to adapt to Luanti scenes. |
| Raz, Cao, Saxe & Frank, [*A stimulus-computable rational model of visual habituation in infants and adults*](https://elifesciences.org/reviewed-preprints/102713) (eLife reviewed preprint, 2025) | RANCH treats looking as rational information sampling over raw images. It fits pre-registered infant and adult habituation data and predicted new dishabituation results. | A formal model to fit the founder's planned habituation curves. |
| Aw, Kotar, Lee et al., [*Zero-shot World Models Are Developmentally Efficient Learners*](https://arxiv.org/abs/2604.10333) (preprint, 2026) | A world model trained on one child's first-person experience, competitive on physical-understanding benchmarks, with abilities emerging in stages. | Staged emergence without reward is the trajectory we want to measure in the founder. |
| Pandey, Lee, Wood & Wood, [*Parallel development of object recognition in newborn chicks and deep neural networks*](https://doi.org/10.1371/journal.pcbi.1012600) (PLOS Computational Biology 20(12), 2024) | Chicks and artificial agents were raised in matching virtual chambers. With the same input, networks developed chick-like view-invariant recognition. | A "digital twin" method: give the animal and the agent the same world and the same tests. |
| Garimella, Pak, Wood & Wood, [*A Newborn Embodied Turing Test for Comparing Object Segmentation Across Animals and Machines*](https://iclr.cc/virtual/2024/poster/17721) (ICLR 2024) | Newborn chicks solved one-shot segmentation; artificial chicks did not, learning background-dependent representations. | Animal-development tests on embodied agents, with a warning that agents can fail where newborns succeed. |
| Vong, Wang, Orhan & Lake, [*Grounded language acquisition through the eyes and ears of a single child*](https://www.science.org/doi/10.1126/science.adi1374) (Science 383, 2024) | A contrastive model trained on 61 hours of one child's head-camera video and speech learned many word-referent mappings and generalised zero-shot. | A single embodied experience stream suffices for grounded concepts, supporting the single-founder design. |
| Orhan & Lake, [*Learning high-level visual representations from a child's perspective without strong inductive biases*](https://www.nature.com/articles/s42256-024-00802-0) (Nature Machine Intelligence 6, 2024) | Models trained on about 200 hours of one child's video, without supervision, learned strong visual representations, consistent across two more children. | Rich representations from realistic first-person input alone. |
| Orhan, Wang, Wang et al., [*Self-supervised learning of video representations from a child's perspective*](https://arxiv.org/abs/2402.00300) (CogSci 2024) | Video models on one child's footage learned action concepts from few labels and more robust object representations than image models. | Temporal prediction from the agent's own stream matters, relevant to the founder's predictive brain. |
| Jiang, Yang, Norman et al., [*Continual Visual and Verbal Learning Through a Child's Egocentric Input*](https://arxiv.org/abs/2606.05115) (preprint, 2026) | One chronological pass over SAYCam with replay and three contrastive losses, closing much of the gap to shuffled offline training. | Learning from a single lifetime-ordered stream, which is the founder's regime. |
| Long, Sparks, Xiang et al., [*The BabyView dataset*](https://arxiv.org/abs/2406.10447) (CCN 2025) | 868 hours of high-resolution head-camera video from children aged 6 months to 3 years, with motion sensors and annotations. | A reference for realistic infant sensorimotor input, including head motion comparable to the founder's proprioception. |
| Tan, Yu, Long et al., [*DevBench: A multimodal developmental benchmark for language learning*](https://arxiv.org/abs/2406.10215) (NeurIPS 2024 Datasets and Benchmarks) | Seven tasks with child and adult data; models are compared to humans by response pattern, not accuracy. | A model for reporting the founder's results against infant data. |
| Li et al., [*Core Knowledge Deficits in Multi-Modal Language Models*](https://arxiv.org/abs/2410.10855) (ICML 2025) | Twelve core-knowledge concepts across 230 models. Early-developing abilities are weaker and scale poorly. | Motivates testing core knowledge directly; a source of concepts for later tests. |
| Wang, Wang, Wang et al., [*BabyVLM-V2: Toward Developmentally Grounded Pretraining and Benchmarking of Vision Foundation Models*](https://arxiv.org/abs/2512.10932) (preprint, accepted to CVPR 2026 per the authors) | Turns the NIH Baby Toolbox into ten AI tasks; a compact model trained from scratch is competitive. | A standardised infant battery for tests beyond habituation, permanence and means-end. |
| Wang, Chandra, Liu et al., [*BabyVLM: Data-Efficient Pretraining of VLMs Inspired by Infant Learning*](https://arxiv.org/abs/2504.09426) (ICCV 2025) | Infant-style benchmarks and a synthetic child-directed training set that beats SAYCam-only training. | Secondary reference for developmental evaluation. |
| Shi, Cai & Stefanov, [*Do Blind Spots Matter for Word-Referent Mapping? A Computational Study with Infant Egocentric Video*](https://arxiv.org/abs/2511.11725) (preprint, 2025) | Masking matched to the eye's blind spot works at least as well as random masking for word learning. | Minor: biologically motivated perceptual constraints. |

### Artificial life with learning agents

| Work | What they did and found | Relation to AI Village |
| --- | --- | --- |
| Lu, Beukman, Matthews & Foerster, [*JaxLife: An Open-Ended Agentic Simulator*](https://arxiv.org/abs/2409.00853) (preprint, 2024) | Embodied neural agents survive in a world with programmable systems; evolved agents show rudimentary communication, agriculture and tool use. Already listed above for licensing. | The nearest ALife platform with learning agents, but evolution- and fitness-driven rather than reward-free. |
| Kumar, Lu, Kirsch et al., [*Automating the Search for Artificial Life with Foundation Models*](https://arxiv.org/abs/2412.17799) (Artificial Life 31(3), 2025) | ASAL uses vision-language models to score simulation videos, finding new Lenia and Boids lifeforms and open-ended cellular automata. | Automated open-endedness metrics we could apply to rendered village runs. |
| Baid, Erlebach, Hellegouarch & Wieser, [*Guiding Evolution of Artificial Life Using Vision-Language Models*](https://arxiv.org/abs/2509.22447) (ALIFE 2025) | A second model proposes new targets from a Lenia simulation's history, trading novelty against coherence. | A VLM as an open-ended critic over simulation footage. |
| Zhang, Risi & Darlow, [*Petri Dish Neural Cellular Automata*](https://pub.sakana.ai/pdnca/) (Sakana AI technical report, 2025) | Competing NCAs keep learning during the simulation. Learning turned steady states into cycles, and cooperation emerged in a competitive setup. | Evidence that lifetime learning, not only evolution, changes ALife dynamics. |
| Faldor & Cully, [*Toward Artificial Open-Ended Evolution within Lenia using Quality-Diversity*](https://arxiv.org/abs/2406.04235) (ALIFE 2024) | MAP-Elites and AURORA with a learned descriptor space evolve diverse Lenia patterns. | Quality-diversity measures of behavioural diversity across village runs. |
| Paolo, Warner, Shahrzad et al., [*TerraLingua: Emergence and Analysis of Open-endedness in LLM Ecologies*](https://arxiv.org/abs/2603.16910) (preprint, 2026) | A persistent ecology of LLM agents with limited resources and lifespans develops norms, division of labour and artifact lineages. | Relevant to the village's later multi-agent stages, though its agents are LLM-driven. |

## Algorithms / designs to borrow into Rust V2

| Source | Borrow into V2 | Module affinity (plan) |
| --- | --- | --- |
| Polyworld | Retinal rays (no labels); metabolic neural/sensor cost; Hebbian / eligibility plasticity concepts; evolvable sensor budgets | `agent/senses`, `agent/brain`, `genetics` |
| ALIEN | Modular organs; localized damage; construction; energy/matter dashboards | `agent/body`, `agent/physiology`, `ecology`, observability |
| Bibites | Digestion delay; organ allocation budgets; pheromone UX; lineage viz; procedural sprites | `physiology`, `ecology`, `sim-app` |
| Mutable | Body-tree morphology *design* (after license) | `agent/body` |
| biosim4 | Compact genome→NN mapping; pheromone fields; selection/logging patterns | `genetics`, `ecology`, `experiments` |
| JaxLife | Cultural / agricultural / tool-use **experiment protocols** and controls | `experiments` |
| Cephalopods | Soft segment / appendage control motifs | `agent/body` |

## Code / licenses that are off-limits (or blocked pending decision)

| Item | Why |
| --- | --- |
| Polyworld source | APSL-2.0 — not deny-allowed; external-deploy source obligations |
| The Bibites binary / decompiled code / assets | Proprietary EULA |
| Mutable source | No license → default all-rights-reserved |
| JaxLife source | No license → do not copy |
| Life Engine source | GPL-3.0 — would force copyleft; not deny-allowed |
| Gene Pool source | Commons Clause — non-commercial restriction |
| ALIEN CUDA kernels as a runtime dependency | Wrong hardware (Intel iGPU); wrong determinism model — even though BSD-3 is permissive |

## Feasibility of local inspection (this audit)

Performed without sudo and without long installs:

- Fetched raw `LICENSE` / `README` / GitHub API metadata for Polyworld, ALIEN, biosim4, Mutable, JaxLife, Cephalopods, Life Engine.
- Confirmed Mutable and JaxLife lack published licenses.
- Confirmed ALIEN CUDA/AMD GPU requirements vs local Intel iGPU.
- Did **not** full-clone ALIEN (large recursive CUDA tree) or install CUDA/JAX stacks.

Optional later (not part of this todo): shallow clone biosim4 or Mutable for read-only browsing once licenses for Mutable are clarified; cloud GPU session for ALIEN demos only.

## Decision for V2 dependency selection

1. **No foreign ALife codebase becomes a Cargo dependency.** Prefer clean-room Rust under MIT/Apache-compatible crates already allowed by `deny.toml`.
2. **Primary idea donors:** Polyworld (sensing/learning/metabolism), ALIEN (organs/damage/conservation UX), Bibites (ecology UX), biosim4 (genome→NN compactness), JaxLife (culture experiment design).
3. **Primary code reference if any:** biosim4 and Cephalopods (MIT) — read and reimplement; attribute if substantial algorithm text is adapted.
4. **Blocked until license:** Mutable (body-tree code), JaxLife (implementation).
5. **Never:** Bibites code; Polyworld/Life Engine/Gene-Pool-Commons as vendored source without an explicit project-wide license change.
