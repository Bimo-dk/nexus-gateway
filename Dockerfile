FROM rust:1-slim AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation separately from source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
COPY static ./static
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# ----------------------------------------------------------------------------
# Configuration — override any of these at `docker run -e KEY=value`
# or in docker-compose under `environment:`.
# ----------------------------------------------------------------------------
# Network
ENV PORT=8668

# Registry connection (REQUIRED — startup fails fast if unset)
ENV REGISTRY_URL=
ENV NEXUS_TOKEN=

# Gate identity (REQUIRED — the domain this gateway instance serves)
ENV NEXUS_GATE_NAME=

# Auto-registration (OPTIONAL — set NEXUS_HOST_NAME to have the gateway
# find-or-create its host and gate in the registry on first boot).
ENV NEXUS_HOST_NAME=
ENV NEXUS_HOST_URL=
ENV NEXUS_HOST_FRAMEWORK=
ENV NEXUS_HOST_REMOTE_ENTRY=
ENV NEXUS_HOST_EXPOSED_MODULE=
ENV NEXUS_GATE_LABEL=

# Observability
#   LOG_JSON=1 switches the tracing layer to JSON output (for log aggregators).
#   RUST_LOG follows the standard env-filter syntax (e.g. "info,nexus_gateway=debug").
ENV LOG_JSON=
ENV RUST_LOG=info
# ----------------------------------------------------------------------------

COPY --from=builder /app/target/release/nexus-gateway /usr/local/bin/nexus-gateway

EXPOSE 8668

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -sf "http://localhost:${PORT}/health" || exit 1

CMD ["nexus-gateway"]
