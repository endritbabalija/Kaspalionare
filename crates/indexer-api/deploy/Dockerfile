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

FROM rust:1.94.0-bookworm AS build
WORKDIR /src
COPY . .
WORKDIR /src/crates
RUN cargo build --locked --release -j 2 -p indexer-api --bin indexer-api \
 && cp target/release/indexer-api /indexer-api

FROM scratch AS export
COPY --from=build /indexer-api /indexer-api

FROM debian:bookworm-slim AS run
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
