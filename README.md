# AI Village

AI Village is a CPU-first artificial-life research project. Its deterministic simulation core
is intentionally independent from Bevy, so the same seeded world can be rendered interactively
or run headlessly for experiments.

## Status: V1 freeze

The current grid/dot simulator is frozen as **V1** — the reproducible baseline before embodied V2
work. Behavior is preserved; golden hashes cover complete canonical state (agents, parentage,
free-list, prior signals, experiment controls, and counters).

| Marker | Value |
| --- | --- |
| Config / format schema | `sim-format` `CONFIG_SCHEMA_VERSION = 1` (`configs/default.toml`) |
| Snapshot header schema | `SNAPSHOT_SCHEMA_VERSION = 1` |
| Checkpoint / event-log schemas | `CHECKPOINT_SCHEMA_VERSION = 1`; `EVENT_LOG_SCHEMA_VERSION = 1` |
| State-hash coverage | `sim_core::STATE_HASH_VERSION = 1` |
| Golden fixtures | `tests/fixtures/v1_golden_replay.toml` |

Reproduce golden hashes:

```sh
cargo run -p sim-cli --locked -- golden
```

## Checkpoints and replay logs

`sim-format::CheckpointFile` stores the exact authoritative `sim-core` state, including stable
slot generations/free-list order, controller state, signals, controls, counters, and grid fields.
Its versioned TOML envelope records the saved tick and canonical hash; loading validates both
before returning a resumable `World`. `sim-format::EventLog` is an append-only sequence of
`tick + state_hash` replay checkpoints.

The headless CLI can write and resume V1 checkpoints:

```sh
cargo run -p sim-cli --locked -- save /tmp/village-v1.toml 500
cargo run -p sim-cli --locked -- resume /tmp/village-v1.toml 500
```

## Workspace

- `sim-core`: authoritative, deterministic simulation domain with no renderer dependency.
- `sim-format`: versioned configurations, snapshots, and event-log data structures.
- `sim-app`: Bevy desktop shell for loading, running, pausing, and inspecting worlds.
- `sim-cli`: headless experiment, soak, and golden-replay verification.

## Prerequisites

The repository pins Rust 1.99.1 with `rustfmt` and Clippy in `rust-toolchain.toml`. On Linux,
install a C toolchain and the platform libraries required by winit/wgpu before building the
desktop application. If the host lacks a C compiler, the Flatpak SDK
`org.freedesktop.Sdk` can provide one for headless `sim-core` / `sim-cli` builds.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked
cargo run -p sim-cli --locked -- golden
cargo doc --workspace --all-features --no-deps
```
