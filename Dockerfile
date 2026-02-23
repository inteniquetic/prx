# syntax=docker/dockerfile:1

FROM rust:1.85-bookworm AS builder

RUN apt-get update \
    && apt-get install --no-install-recommends -y \
        cmake \
        g++ \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Temporary build-time fix for vendored pingora-core on Linux/libc.
RUN sed -i 's/let base_group: libc::c_int =/let base_group: libc::gid_t =/' \
    vendor/pingora-core/src/server/daemon.rs

# Use --locked for deterministic production builds.
RUN cargo build --release --locked --bin prx

FROM debian:bookworm-slim AS release

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 prx \
    && useradd --system --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin prx

WORKDIR /app
COPY --from=builder /src/target/release/prx /app/prx

RUN chown -R prx:prx /app
USER 10001:10001

# prx defaults to 8080 and can optionally enable TLS on 8443.
EXPOSE 8080 8443

ENTRYPOINT ["/app/prx"]
