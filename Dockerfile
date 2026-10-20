# syntax=docker/dockerfile:1
# Headless images for sim-cli experiments and golden-replay verification.
# sim-app (Bevy) is not built here; it needs a display and runs natively on the host.
#
# Targets:
#   runtime      CPU-only (default)
#   runtime-gpu  adds the `gpu` feature and the Vulkan loader; build with
#                --build-arg CARGO_FEATURES=gpu --target runtime-gpu, run with an NVIDIA GPU

ARG RUST_VERSION=1.99.1
ARG DEBIAN_RELEASE=trixie

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS build
ARG CARGO_FEATURES=""
WORKDIR /app
COPY . .
# Cache mounts keep the registry and target dir between builds; the binary is copied out
# inside the same step because the mounted target dir is not part of the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build -p sim-cli --profile release-optimized --locked \
        ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} \
    && cp target/release-optimized/sim-cli /usr/local/bin/sim-cli

FROM debian:${DEBIAN_RELEASE}-slim AS runtime-base
RUN useradd --create-home --uid 10001 village
# `golden` resolves fixtures relative to the build-time manifest dir
# (/app/crates/sim-cli/../../tests/fixtures), so keep that layout.
WORKDIR /app
COPY --from=build /app/tests/fixtures tests/fixtures
COPY --from=build /app/configs configs
RUN mkdir -p crates/sim-cli /out && chown village /out
COPY --from=build /usr/local/bin/sim-cli /usr/local/bin/sim-cli
VOLUME ["/out"]
ENTRYPOINT ["sim-cli"]

FROM runtime-base AS runtime-gpu
RUN apt-get update \
    && apt-get install -y --no-install-recommends libvulkan1 \
    && rm -rf /var/lib/apt/lists/*
# The `graphics` capability makes the NVIDIA runtime mount the Vulkan ICD into the container.
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,graphics
USER village
CMD ["gpu-verify"]

FROM runtime-base AS runtime
USER village
CMD ["help"]
