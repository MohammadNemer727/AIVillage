# Third-party artificial-life attributions

This file records licenses and reuse decisions for external artificial-life projects that inform Embodied AI Village. It is not a substitute for legal advice. Update this file whenever an algorithm is adapted from a licensed source, or when a previously unlicensed project publishes a license.

Companion analysis: [`docs/prior-art.md`](docs/prior-art.md).

## Policy (this repository)

- Cargo / crate licenses must remain within `deny.toml` allow-list: Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MIT, Unicode-3.0, Zlib.
- Prefer **ideas and clean-room reimplementation** over vendoring ALife simulator source.
- Do not add GPL, APSL, Commons-Clause, or unlicensed source as dependencies or copied trees without an explicit project-license decision.
- Ideas, experimental protocols, and publicly described algorithms may be reimplemented; cite the origin here when substantial.

## Inventoried projects

### Polyworld

| | |
| --- | --- |
| Upstream | https://github.com/polyworld/polyworld |
| License | Apple Public Source License 2.0 (`LICENSE.txt`) |
| Code in this repo | **None** |
| Allowed reuse | Conceptual: retinal sensing, energy-priced neural activity, Hebbian lifetime learning, evolvable morphology/neural traits |
| Not allowed | Copying or linking APSL Covered Code without accepting APSL obligations and a project license decision |

### ALIEN

| | |
| --- | --- |
| Upstream | https://github.com/chrxh/alien |
| License | BSD 3-Clause — Copyright (c) 2023, Christian Heinemann |
| Code in this repo | **None** |
| Allowed reuse | Conceptual: modular body parts, damage, energy conservation, construction, ecosystem dashboards. BSD permits code reuse **with** copyright notice if we ever port an algorithm; prefer clean-room Rust. |
| Hardware note | Requires NVIDIA CUDA or supported AMD GPU. **This laptop (Intel iGPU only) cannot run ALIEN.** |
| Not allowed | Treating CUDA kernels as a runtime dependency of this CPU-deterministic project |

### The Bibites

| | |
| --- | --- |
| Upstream | https://thebibites.itch.io/the-bibites — Omnia Studios EULA |
| License | Proprietary; no open-source grant |
| Code in this repo | **None** |
| Allowed reuse | Behavioral / design references only (digestion, organs, pheromones, lineage UI, procedural appearance) |
| Not allowed | Reverse engineering, decompilation, copying assets, derivative works from the game binary |

### Mutable

| | |
| --- | --- |
| Upstream | https://github.com/Ashwagandhae/mutable |
| License | **None published** (no `LICENSE` file as of audit 2026-10-20) |
| Code in this repo | **None** |
| Allowed reuse | High-level design discussion only (organism body trees; plant/animal continuum) |
| Not allowed | Copying source until a permissive license or written permission exists |

### biosim4

| | |
| --- | --- |
| Upstream | https://github.com/davidrmiller/biosim4 |
| License | MIT License — Copyright (c) 2021-2022 David R. Miller and other contributors |
| Code in this repo | **None** (reference only) |
| Allowed reuse | May study and adapt algorithms under MIT terms; retain copyright notice if substantial code or documentation text is used |
| Preferred practice | Reimplement genome→NN / pheromone / logging ideas in idiomatic Rust rather than a wholesale port |

### JaxLife

| | |
| --- | --- |
| Upstream | https://github.com/luchris429/JaxLife — paper arXiv:2409.00853 |
| License | **None published** on the repository as of audit 2026-10-20 |
| Code in this repo | **None** |
| Allowed reuse | Paper-described experiment designs (agriculture, tools, communication, cultural accumulation controls); cite the paper |
| Not allowed | Copying repository source without a license grant |

### Cephalopods (secondary)

| | |
| --- | --- |
| Upstream | https://github.com/jobtalle/cephalopods |
| License | MIT — Copyright (c) 2020 Job Talle |
| Code in this repo | **None** |
| Allowed reuse | Morphology / soft-body control ideas; MIT notice if code is adapted |

### The Life Engine (secondary)

| | |
| --- | --- |
| Upstream | https://github.com/MaxRobinsonTheGreat/LifeEngine |
| License | GPL-3.0 |
| Code in this repo | **None** |
| Allowed reuse | Conceptual cell-role ideas only |
| Not allowed | Copying GPL source into this tree / shipping as a combined work without GPL compliance |

### Gene Pool / Swimbots (secondary)

| | |
| --- | --- |
| Upstream | https://www.swimbots.com/code.html |
| License | MIT with Commons Clause (non-commercial; no corporate entity use per upstream statement) |
| Code in this repo | **None** |
| Allowed reuse | Conceptual swimming morphology / sexual selection ideas |
| Not allowed | Incorporating Commons-Clause code into this project |

## Reuse ledger (fill as V2 implements)

| Date | Component | Origin | Form of reuse | License action |
| --- | --- | --- | --- | --- |
| 2026-10-20 | — | Prior-art audit | Documentation only | Created this file and `docs/prior-art.md` |

When adapting MIT/BSD algorithm text or tables, add a row and ensure notices remain accurate.

## Recommended borrow vs off-limits (quick reference)

**Borrow (ideas → clean-room Rust V2)**

- Polyworld: retina, metabolic neural cost, Hebbian/plasticity concepts, evolvable sensors.
- ALIEN: modular organs, damage, conservation dashboards (CPU reimplementation).
- Bibites: digestion, organs, pheromones, lineage/procedural UX (design only).
- biosim4: compact genome→NN, pheromones, experiment logging patterns (MIT-aware).
- JaxLife: culture / agriculture / tool-use *experiment protocols* (cite paper).

**Off-limits / blocked**

- Bibites code and decompilation.
- Polyworld / Life Engine / Gene-Pool-Commons source trees as vendored code.
- Mutable and JaxLife source until licensed.
- ALIEN as a local runtime on Intel-iGPU machines.
