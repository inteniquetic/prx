# syntax=docker/dockerfile:1

FROM rust:1.93-bookworm AS builder

RUN apt-get update \
    && apt-get install --no-install-recommends -y \
        cmake \
        g++ \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Use --locked for deterministic production builds.
RUN cargo build --release --locked --bin prx

FROM debian:bookworm-slim AS release

# libssl3 is required at runtime: prx builds pingora with its openssl TLS
# backend, without which the TLS listener cannot complete a handshake.
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates libssl3 \
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
