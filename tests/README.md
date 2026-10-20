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
