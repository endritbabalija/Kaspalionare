# The hosted indexer API (F6 part C; docs/launch/indexer-server.md): a reproducible Linux build of
# `indexer-api` (crates/indexer-api) in the one-process mode (`--node`: the M8 indexer follows a testnet-10
# node into the store, the API serves it, with node-built anchor proofs).
#
# Build from the repository root (the context is the whole repository; Dockerfile.dockerignore keeps it small):
#   docker build -f crates/indexer-api/deploy/Dockerfile -t kaspalionare-indexer .
# Copy the binary out instead of running the image (the systemd option):
#   docker build -f crates/indexer-api/deploy/Dockerfile --target export --output type=local,dest=out .
#
# Pinned: Rust 1.94.0 (crates/rust-toolchain.toml), Debian 12 (bookworm), Cargo.lock (--locked), rusty-kaspa
# v2.1.0 and SilverScript v1.0.0 by git revision (crates/Cargo.toml). Two jobs, so a 4 GB machine builds it.
#
# Base images are pinned by digest (F6 security review; looked up 2026-10-28 on Docker Hub, the multi-platform index
# digests). A tag can be moved to other contents; a digest cannot. To update one: `docker buildx imagetools inspect
# rust:1.94.0-bookworm` (or `debian:bookworm-slim`) prints the current "Digest:"; put it after the tag's `@`, rebuild,
# and rerun the image check in docs/launch/indexer-server.md. The Debian packages (ca-certificates) still come from
# the pinned image's apt sources at build time.

FROM rust:1.94.0-bookworm@sha256:365468470075493dc4583f47387001854321c5a8583ea9604b297e67f01c5a4f AS build
WORKDIR /src
COPY . .
WORKDIR /src/crates
RUN cargo build --locked --release -j 2 -p indexer-api --bin indexer-api \
 && cp target/release/indexer-api /indexer-api

FROM scratch AS export
COPY --from=build /indexer-api /indexer-api

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS run
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data --shell /usr/sbin/nologin indexer \
 && mkdir -p /data && chown indexer /data
COPY --from=build /indexer-api /usr/local/bin/indexer-api
USER indexer
VOLUME /data
# Flags come from INDEXER_API_* variables (crates/indexer-api/src/config.rs); see indexer-api.env.example.
ENV INDEXER_API_STORE=/data \
    INDEXER_API_BIND=127.0.0.1:8790 \
    INDEXER_API_CLIENT_IP=forwarded
EXPOSE 8790
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/indexer-api"]
