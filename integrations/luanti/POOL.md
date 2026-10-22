# Luanti population client pool

Player-body mode uses one real windowless Luanti client per occupied physical body slot and one
Rust population coordinator for the whole cohort. The default remains the legacy-compatible
single client named `founder`. `PLAY_FOUNDER_COUNT` sets the initial cohort, while
`PLAY_POPULATION_CAPACITY` starts enough reusable `founder-NN` body clients to receive later
births. Setting both to 4 (or `BODY_TEST_FOUNDER_COUNT=4`) starts `founder-00` through
`founder-03`.

All clients post to one loopback endpoint. For the initial cohort:

- player `founder-NN` occupies reusable `body_slot = NN`;
- that slot initially hosts never-reused evolving `agent_id = NN + 1`;
- Rust routes by `agent_id` and rejects a mismatched `body_slot`;
- each Lua body owns its sequence, pending request token, action, senses, transport status and
  HUD state, while Rust owns an independent runtime/checkpoint for each individual;
- timeout, rejection, disconnect and late-callback handling mutates only the affected Lua body.

For populations, `SIM_LUANTI_POPULATION` names the durable coordinator directory,
`SIM_LUANTI_FOUNDER_COUNT` creates the initial cohort, and
`SIM_LUANTI_POPULATION_CAPACITY` sets available slots. The manifest records current
individual-to-slot occupancy and lineage; `individuals/<agent_id>.json` stores lifetime state.
Single-founder launch remains compatible with the old `SIM_LUANTI_STATE` file.

## Autonomous lifecycle

Real VoxeLibre player death posts `POST /v1/lifecycle` with a durable event ID and frees only that
individual's slot. It never requests a birth or respawns the dead individual. A living body whose
policy emits a positive first blank signal independently requests reproduction. Rust accepts a
birth only when all of these hold:

- the parent still occupies the claimed slot and its last accepted action made that decision;
- `SIM_LUANTI_REPRODUCTIVE_MATURITY` decisions have elapsed;
- reserve is at least `SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE`;
- `SIM_LUANTI_REPRODUCTIVE_COST` can be charged; and
- a body slot is free.

The default values are 100 decisions, reserve 160, and cost 80. Reserve follows felt fullness,
recovers slowly while the body remains sufficiently full, and the birth cost is durable. Denials
are reported as `no_free_slot`, `biological_eligibility`, or `infrastructure`. A successful
response rebinds the free `founder-NN` body to the newborn ID with sequence zero. `GET
/v1/population` resynchronizes occupancy and next sequence after restart.

Lifecycle requests are retained in Luanti mod storage until acknowledged. Rust keeps each result
by event ID and uses `lifecycle.wal` to roll a transaction forward after a crash; parent cost,
lineage, newborn checkpoint, and slot assignment are therefore exactly once.

## Experiment 001

Run either unattended four-founder cohort:

```sh
integrations/luanti/experiment-001.sh enabled
integrations/luanti/experiment-001.sh disabled
```

Each stops at 100 births, natural extinction, or
`EXPERIMENT_001_DEADLINE_SECONDS` (default one day) and writes a JSON report in its world folder.
Set `EXPERIMENT_001_*` variables to change the bounds and eligibility settings. The report labels
canonical recovery as disabled and excluded. No recovery is performed on extinction.

The short natural gate used before a long cohort is:

```sh
EXPERIMENT_001_FOUNDERS=3 EXPERIMENT_001_CAPACITY=4 \
EXPERIMENT_001_BIRTHS=1 EXPERIMENT_001_DEADLINE_SECONDS=300 \
integrations/luanti/experiment-001.sh enabled pilot-world pilot-report.json
```

It retains `bridge-trace.ndjson`, a create-once `run-provenance.json`, transition eligibility
snapshots, queued lifecycle attempts, and final results. It is natural evidence only when the
provenance says `forced_signal: false`; forced wiring checks are excluded.
