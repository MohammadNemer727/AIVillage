#!/usr/bin/env sh
# Rebuilds crates/sim-gpu/shaders/sim_shaders.spv from sim-kernels with rust-gpu, and records a
# fingerprint of the sources it was built from.
#
#   build-shaders.sh          rebuild the .spv and its fingerprint (needs cargo-gpu)
#   build-shaders.sh --check  fail if kernel or shader sources changed since the last rebuild
#
# Run without arguments after any change to sim-kernels or sim-shaders, then commit both outputs.
# Requires cargo-gpu (`cargo install cargo-gpu --version 0.11.0 --locked`); the first run
# installs its pinned nightly toolchain and SPIR-V codegen backend. `--check` needs only a POSIX
# shell and sha256sum, so stable-only CI can catch a stale shader. On Fedora hosts whose cc/c++
# commands are non-GNU wrappers, use build-shaders-container.sh for the native GNU build.
set -eu
cd "$(dirname "$0")"
output=../sim-gpu/shaders
fingerprint_file="$output/sources.sha256"

# Hashes every input with CRLF normalised, so Windows and Linux checkouts agree.
fingerprint() {
    for file in $(find ../sim-kernels/src src -name '*.rs' | sort) \
        ../sim-kernels/Cargo.toml Cargo.toml Cargo.lock build-shaders.sh \
        build-shaders-container.sh Dockerfile; do
        printf '%s %s\n' "$file" "$(tr -d '\r' < "$file" | sha256sum | cut -d' ' -f1)"
    done | sha256sum | cut -d' ' -f1
}

if [ "${1:-}" = "--check" ]; then
    expected=$(cat "$fingerprint_file")
    actual=$(fingerprint)
    if [ "$expected" != "$actual" ]; then
        echo "sim_shaders.spv is stale: kernel or shader sources changed." >&2
        echo "Rebuild with crates/sim-shaders/build-shaders.sh and commit the result." >&2
        exit 1
    fi
    echo "sim_shaders.spv matches its sources ($actual)"
    exit 0
fi

cargo gpu build \
    --auto-install-rust-toolchain \
    --shader-crate . \
    --output-dir "$output" \
    --capabilities Int8 \
    --capabilities Int16
fingerprint > "$fingerprint_file"
echo "wrote $output/sim_shaders.spv and $fingerprint_file"
