#!/usr/bin/env sh
# Builds the rust-gpu shader with Fedora's native GNU toolchain even when host compiler commands
# are wrappers for Zig or another incompatible linker. Uses the host's pinned cargo-gpu/rustup.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$here/../.." && pwd)
runtime=${SHADER_CONTAINER_RUNTIME:-podman}
image=${SHADER_CONTAINER_IMAGE:-ai-village-shader-builder}
mkdir -p "$HOME/.cache/rust-gpu"

case "$runtime" in
  podman) user_args="--userns=keep-id --user $(id -u):$(id -g)" ;;
  docker) user_args="--user $(id -u):$(id -g)" ;;
  *) echo "SHADER_CONTAINER_RUNTIME must be podman or docker" >&2; exit 2 ;;
esac

"$runtime" build -t "$image" -f "$here/Dockerfile" "$here"
# shellcheck disable=SC2086 # user_args intentionally contains multiple runtime arguments.
"$runtime" run --rm $user_args \
  -e HOME="$HOME" \
  -e PATH="$HOME/.cargo/bin:/usr/bin:/bin" \
  -e CARGO_HOME="$HOME/.cargo" \
  -e RUSTUP_HOME="$HOME/.rustup" \
  -e CC=/usr/bin/gcc \
  -e CXX=/usr/bin/g++ \
  -v "$repo:$repo:z" \
  -v "$HOME/.cargo:$HOME/.cargo:z" \
  -v "$HOME/.rustup:$HOME/.rustup:z" \
  -v "$HOME/.cache/rust-gpu:$HOME/.cache/rust-gpu:z" \
  -w "$repo" \
  "$image" sh crates/sim-shaders/build-shaders.sh
