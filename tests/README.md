# End-to-end tests and fixtures

Cross-crate deterministic replay scenarios and frozen V1 golden hashes live here. Unit tests remain
next to the simulation primitives they exercise.

## V1 golden replay

File: [`fixtures/v1_golden_replay.toml`](fixtures/v1_golden_replay.toml)

Each case pins `seed`, world geometry, and expected `state_hash` values at selected ticks under
`sim-format` schema version 1 and `sim_core::STATE_HASH_VERSION`.

Reproduce / verify with the headless CLI:

```sh
cargo run -p sim-cli --locked -- golden
cargo run -p sim-cli --locked -- golden-print   # regenerate hash lines after intentional hash-schema bumps
```

The same fixture is also asserted by `crates/sim-core/tests/v1_golden_replay.rs`.

## One-agent runtime golden trace

File: [`fixtures/agent_runtime_golden.ndjson`](fixtures/agent_runtime_golden.ndjson)

One `EnvironmentTransition` per line: a seeded `FounderSpec` driven by the deterministic
`FakeEnvironment`, with every observation, primitive action, and post-decision brain fingerprint.
`crates/sim-format/tests/agent_runtime_golden.rs` asserts that a live run reproduces the file byte
for byte, that replaying the recorded observations reproduces every decision, and that resuming from
a TOML `AgentCheckpoint` midway reproduces the rest.

Regenerate only after an intentional behaviour change:

```sh
UPDATE_AGENT_GOLDEN=1 cargo test -p sim-format --test agent_runtime_golden
```
