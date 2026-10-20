# syntax=docker/dockerfile:1
# Headless, CPU-only image for sim-cli experiments and golden-replay verification.
# sim-app (Bevy) is not built here; it needs a display and runs natively on the host.

ARG RUST_VERSION=1.99.1
ARG DEBIAN_RELEASE=trixie

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS build
WORKDIR /app
COPY . .
# Cache mounts keep the registry and target dir between builds; the binary is copied out
# inside the same step because the mounted target dir is not part of the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build -p sim-cli --profile release-optimized --locked \
    && cp target/release-optimized/sim-cli /usr/local/bin/sim-cli

FROM debian:${DEBIAN_RELEASE}-slim AS runtime
RUN useradd --create-home --uid 10001 village
# `golden` resolves fixtures relative to the build-time manifest dir
# (/app/crates/sim-cli/../../tests/fixtures), so keep that layout.
WORKDIR /app
COPY --from=build /app/tests/fixtures tests/fixtures
COPY --from=build /app/configs configs
RUN mkdir -p crates/sim-cli /out && chown village /out
COPY --from=build /usr/local/bin/sim-cli /usr/local/bin/sim-cli
USER village
VOLUME ["/out"]
ENTRYPOINT ["sim-cli"]
CMD ["help"]
