# Final acceptance verification

Date: 2026-10-20  
Environment: Rust 1.99.1 hosted inside the Flatpak `org.freedesktop.Sdk//25.08`
linker environment; all dependency resolution was offline from the locked cache. No GUI was run.

## Determinism and persistence

- The 1,000-tick deterministic replay completed with final hash `a87fe27e21f5c63c`.
- A default V1 checkpoint at tick 500 (`ba3d81abdfc921d1`) resumed to tick 1,000 with
  `7a354a3c77bead57`, exactly matching an uninterrupted tick-1,000 run.
- The extended format regression test checkpoints after 500 ticks and compares 500 resumed
  ticks against uninterrupted history.
- All V1 golden replay fixture hashes matched.

## Invariants and replicated gates

- The full workspace suite passed: 42 `sim-core` tests, 1 V1 fixture integration test, and
  2 `sim-format` tests. It includes replay, fixed-point, conservation, ecology-bound, and
  configuration/control invariants.
- Eight-seed development, unknown-hazard, physical-social-learning, and cultural-evolution
  CLI gate suites all passed.
- The deterministic eight-replicate observability archive is
  `target/observability-report.toml` (1,200 ticks each; elapsed 16,640 ms).

## Scale result

Development-profile headless population benchmarks (2,000 ticks) completed at:

- 128 requested population: 471.5 ticks/s
- 500 requested population: 83.5 ticks/s
- 2,000 requested population: 31.0 ticks/s

The 2,000-agent, 20,000-tick soak completed in 577.595 s without invariant failure
(`31d16cac705bcac4` → `8d63eb2237e786ce`).

## Quality gates

`cargo fmt --all -- --check`, strict workspace Clippy
(`-D warnings`), and `cargo test --workspace --all-features --locked` passed.
