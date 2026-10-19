# syntax=docker/dockerfile:1
FROM rust:1.99.1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates perl binaryen && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY scripts/install-dioxus.sh /usr/local/bin/install-dioxus
RUN install-dioxus /usr/local/bin && rustup target add wasm32-unknown-unknown
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Production web assets require an authenticated session before reading local data.
RUN MEAL_REQUIRE_AUTH=true dx build --platform web -p meal-app --release --debug-symbols false --locked
RUN cargo build -p meal-server --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && useradd --uid 10001 --create-home app
COPY --from=build /src/target/release/meal-server /usr/local/bin/meal-server
COPY --from=build /src/target/dx/meal-app/release/web/public /app/public
WORKDIR /app
ENV APP_ENV=production BIND_ADDR=0.0.0.0:8081 WEB_DIST=/app/public RUST_LOG=meal_server=info
USER 10001:10001
EXPOSE 8081
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD curl --fail --silent http://127.0.0.1:8081/api/ready || exit 1
ENTRYPOINT ["meal-server"]
