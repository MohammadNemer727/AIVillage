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
  Sensing and thinking run in parallel with rayon; results never depend on thread count.
- `sim-kernels`: `no_std`, allocation-free, integer-only per-agent kernels (controller,
  predictive brain, receptor transforms). Written once and compiled for both CPU and GPU;
  `sim-core` owns the world and calls these with explicit per-agent state.
- `sim-gpu`: wgpu host that runs the kernels on a Vulkan GPU, bit-identical to the CPU.
- `sim-shaders`: rust-gpu entry points wrapping `sim-kernels`; built separately (see below).
- `sim-format`: versioned configurations, snapshots, and event-log data structures.
- `sim-app`: Bevy desktop shell for loading, running, pausing, and inspecting worlds.
- `sim-cli`: headless experiment, soak, and golden-replay verification.

## CPU and GPU from one source

Simulation logic lives once, in `sim-kernels`. The CPU calls it directly; rust-gpu compiles the
same source to SPIR-V for the GPU. Because the kernels use integer arithmetic only, the GPU
produces bit-identical results, and `World::step_with_brains` with the GPU backend reproduces
`World::step`'s state hash on every tick.

Kernel code follows a few rules so it keeps compiling for both targets (documented in
`crates/sim-kernels/src/lib.rs`): fixed-size arrays traversed with indexed loops, no slice
iterators or sub-slices, no allocation, no panics, and integer math only.

After changing `sim-kernels` or `sim-shaders`, regenerate and commit the shader:

```sh
cargo install cargo-gpu --version 0.11.0 --locked   # once
sh crates/sim-shaders/build-shaders.sh              # rebuilds sim-gpu/shaders/sim_shaders.spv
```

CI runs `build-shaders.sh --check`, which fails if the committed shader is stale.

Verify the GPU against the CPU on your machine (needs a Vulkan GPU):

```sh
AI_VILLAGE_REQUIRE_GPU=1 cargo test -p sim-gpu --release
cargo run -p sim-cli --profile release-optimized --features gpu -- gpu-verify 500
```

**Performance today:** the GPU path is correct but not yet faster. With ~1,400 V2 agents on
an RTX 4060 Ti it runs 36.5 ticks/s versus 53.1 ticks/s on a 6-core CPU, because a 64-neuron
brain is cheap and each tick pays for upload, dispatch, and readback. The GPU becomes
worthwhile with much larger populations, heavier brains, or once sensing also moves on-device
and state stays resident on the GPU between ticks.

## Prerequisites

The repository pins Rust 1.99.1 with `rustfmt` and Clippy in `rust-toolchain.toml`. On Linux,
install a C toolchain and the platform libraries required by winit/wgpu before building the
desktop application. If the host lacks a C compiler, the Flatpak SDK
`org.freedesktop.Sdk` can provide one for headless `sim-core` / `sim-cli` builds.

## Docker (headless)

The images build only `sim-cli` (no Bevy, no display). Results match a native build bit for bit.

```sh
docker compose build sim
docker compose run --rm sim golden
docker compose run --rm sim save /out/village-v1.toml 500   # written to ./out on the host
docker compose run --rm sim resume /out/village-v1.toml 500
```

The `gpu` profile adds GPU brains and runs `gpu-verify`. It needs a Linux host with an NVIDIA
driver and the NVIDIA Container Toolkit; Docker Desktop on Windows does not expose Vulkan to
containers, so on Windows run the GPU path natively instead.

```sh
docker compose --profile gpu run --rm sim-gpu                    # gpu-verify, 500 ticks
docker compose --profile gpu run --rm sim-gpu gpu-verify 2000
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked
cargo run -p sim-cli --locked -- golden
cargo doc --workspace --all-features --no-deps
```
