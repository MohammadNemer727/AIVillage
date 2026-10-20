#!/usr/bin/env sh
# Rebuilds crates/sim-gpu/shaders/sim_shaders.spv from sim-kernels with rust-gpu.
#
# Run after any change to sim-kernels, then commit the regenerated .spv. Requires cargo-gpu:
#   cargo install cargo-gpu --version 0.11.0 --locked
# The first run installs cargo-gpu's pinned nightly toolchain and SPIR-V codegen backend.
set -eu
cd "$(dirname "$0")"
cargo gpu build \
    --auto-install-rust-toolchain \
    --shader-crate . \
    --output-dir ../sim-gpu/shaders \
    --capabilities Int8 \
    --capabilities Int16
