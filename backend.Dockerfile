FROM rust:1.100-bookworm AS api-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY openapi.yaml ./openapi.yaml
COPY src ./src
RUN cargo build --release --locked --bin equipment-reservation-api

FROM debian:bookworm-slim AS api
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home app
COPY --from=api-builder /app/target/release/equipment-reservation-api /usr/local/bin/equipment-reservation-api
USER app
ENV APP_ENV=production PORT=8080
EXPOSE 8080
ENTRYPOINT ["equipment-reservation-api"]
