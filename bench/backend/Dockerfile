FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:stable-slim
COPY --from=build /src/target/release/prx-bench-backend /usr/local/bin/prx-bench-backend
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/prx-bench-backend"]
