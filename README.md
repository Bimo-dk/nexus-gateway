# nexus-gateway

Public ingress for a Nexus deployment. Rust binary (axum + hyper + tokio) — fetches its routing table from the registry, proxies the configured host and remotes, enforces seven-layer DDoS protection, and serves a framework-aware SPA shim while remotes load.

- Image: [`ghcr.io/bimo-dk/nexus-gateway`](https://github.com/Bimo-dk/nexus-gateway/pkgs/container/nexus-gateway)
- Listens on: `:8668`
- Trust boundary: **public entry.** The gateway is the only service end users talk to. The registry, hosts and remotes it proxies all live on the internal network. See [security — Network trust boundary](https://nexus.bimo.dk/reference/security#network-trust-boundary).
- Requires: a reachable [nexus-registry](https://github.com/Bimo-dk/nexus-registry)
- Docs: [Tenant-facing overview](https://nexus.bimo.dk/infrastructure/infra-gateway) · [Internals — architecture](https://nexus.bimo.dk/internals/nexus-gateway/architecture) · [code map](https://nexus.bimo.dk/internals/nexus-gateway/code-map)

## Quick start (pull and run)

The gateway needs a registry already running. The minimum to bring one online:

```bash
docker pull ghcr.io/bimo-dk/nexus-gateway:latest

docker run -d \
  --name nexus-gateway \
  -p 8668:8668 \
  -e NEXUS_TOKEN="your-registry-token" \
  -e REGISTRY_URL="http://registry-host:8670" \
  -e NEXUS_GATE_NAME="shop.example.com" \
  ghcr.io/bimo-dk/nexus-gateway:latest

curl http://localhost:8668/health
```

If the gate `shop.example.com` already exists in the registry (with a host attached), the gateway picks it up and starts proxying. If not, see the auto-registration block below.

### First-boot auto-registration

If you're booting the gateway against an empty registry and don't want to seed it manually first, set the optional `NEXUS_HOST_*` variables. The gateway will find-or-create the host **and** the gate on first boot:

```bash
docker run -d \
  --name nexus-gateway \
  -p 8668:8668 \
  -e NEXUS_TOKEN="your-registry-token" \
  -e REGISTRY_URL="http://registry-host:8670" \
  -e NEXUS_GATE_NAME="shop.example.com" \
  -e NEXUS_HOST_NAME="storefront" \
  -e NEXUS_HOST_URL="http://storefront:4200" \
  -e NEXUS_HOST_FRAMEWORK="angular" \
  -e NEXUS_HOST_REMOTE_ENTRY="/host/remoteEntry.json" \
  -e NEXUS_HOST_EXPOSED_MODULE="./AppShell" \
  ghcr.io/bimo-dk/nexus-gateway:latest
```

After first boot the host and gate exist in the registry — the auto-registration block becomes a no-op on subsequent restarts.

## Environment variables

`docker inspect ghcr.io/bimo-dk/nexus-gateway:latest --format '{{json .Config.Env}}'` lists the live contract. The same set, annotated:

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `NEXUS_TOKEN` | **yes** | (empty) | Shared secret matching the registry's `NEXUS_TOKEN`. Used for all registry HTTP and the WebSocket subscribe. |
| `REGISTRY_URL` | **yes** | (empty) | Base URL of the registry, e.g. `http://registry:8670` or `https://nexus.example.com`. |
| `NEXUS_GATE_NAME` | **yes** | (empty) | The gate domain this instance serves. The registry indexes gates by this value. |
| `PORT` | no | `8668` | Listen port. HEALTHCHECK respects overrides. |
| `NEXUS_HOST_NAME` | no | (empty) | Set to enable first-boot auto-registration of the host. The five `NEXUS_HOST_*` variables below are required when this is set. |
| `NEXUS_HOST_URL` | conditional | (empty) | Required when `NEXUS_HOST_NAME` is set. Upstream URL the gateway proxies to for `/host/*`. |
| `NEXUS_HOST_FRAMEWORK` | no | `angular` | One of `angular`, `vue`, `react`, `auto`. Determines which adapter the SPA shim loads. |
| `NEXUS_HOST_REMOTE_ENTRY` | no | `/host/remoteEntry.json` | Path or URL to the host's federation manifest. |
| `NEXUS_HOST_EXPOSED_MODULE` | no | `./AppShell` | Exposed module name on the host's `remoteEntry.json`. |
| `NEXUS_GATE_LABEL` | no | derived from host name or gate domain | Friendly label stored alongside the gate (visible in the portal). |
| `LOG_JSON` | no | (empty → human format) | Set to `1` or `true` to switch the tracing layer to JSON output for log aggregators. |
| `RUST_LOG` | no | `info` | `tracing-subscriber` filter directive (e.g. `info,nexus_gateway=debug`). |

Full reference: [docs — reference/environment](https://nexus.bimo.dk/reference/environment).

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/health` | Liveness probe (also wired to the Docker `HEALTHCHECK`). |
| `GET` | `/metrics` | Prometheus exporter (text format). |
| `GET` | `/ws` | Proxies the registry WebSocket to the browser. |
| `GET/POST/DELETE` | `/api/protection/*` | Manage IP bans. Requires `X-Nexus-Token`. |

Every other path is matched against the registry-built route table; misses fall through to the framework-aware SPA shim.

## Running with a registry

Minimal docker-compose for the pair (gateway + registry):

```yaml
services:
  registry:
    image: ghcr.io/bimo-dk/nexus-registry:latest
    ports: ["8670:8670"]
    volumes:
      - nexus-registry-data:/app/data
    environment:
      NEXUS_TOKEN: ${NEXUS_TOKEN:?set NEXUS_TOKEN in your .env}
      NEXUS_TOKEN_PEPPER: ${NEXUS_TOKEN_PEPPER:?set NEXUS_TOKEN_PEPPER in your .env}
      ALLOWED_ORIGINS: "*"
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8670/health"]
      interval: 10s
      timeout: 5s
      retries: 5

  gateway:
    image: ghcr.io/bimo-dk/nexus-gateway:latest
    ports: ["8668:8668"]
    depends_on:
      registry:
        condition: service_healthy
    environment:
      NEXUS_TOKEN: ${NEXUS_TOKEN}
      REGISTRY_URL: http://registry:8670
      NEXUS_GATE_NAME: localhost:8668
      # Auto-register the host on first boot — drop these once the registry
      # is seeded manually or by the portal.
      NEXUS_HOST_NAME: storefront
      NEXUS_HOST_URL: http://host:4200
      NEXUS_HOST_FRAMEWORK: angular

volumes:
  nexus-registry-data:
```

Set `NEXUS_TOKEN` and `NEXUS_TOKEN_PEPPER` in a sibling `.env`. `docker compose up -d` brings the pair online.

## Build from source

```bash
cargo build --release
NEXUS_TOKEN=dev-token \
REGISTRY_URL=http://localhost:8670 \
NEXUS_GATE_NAME=localhost:8668 \
  ./target/release/nexus-gateway
```

Tests (`wiremock`-backed integration coverage of bootstrap, the seven protection layers, route table semantics, SPA injection escaping, and headers policy):

```bash
cargo test
```
