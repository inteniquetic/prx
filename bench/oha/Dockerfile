# oha pinned to the host version so in-network h1-close runs stay comparable.
FROM rust:1.93-bookworm AS build
RUN cargo install oha --version 1.16.0 --locked

FROM debian:bookworm-slim
COPY --from=build /usr/local/cargo/bin/oha /usr/local/bin/oha
ENTRYPOINT ["oha"]
