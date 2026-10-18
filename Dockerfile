FROM rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets
ARG CARGO_BUILD_JOBS=2
RUN cargo build --locked --release

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin explorer
COPY --from=build /app/target/release/nexawal-explorer /usr/local/bin/nexawal-explorer
USER 10001:10001
ENV EXPLORER_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/nexawal-explorer"]
