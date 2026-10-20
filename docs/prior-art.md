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
