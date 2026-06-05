# nexus-gateway

Public ingress for a Nexus deployment. Rust binary (axum + hyper + tokio) — listens on `:8668`, fetches its config from the registry, proxies the host and remotes, enforces seven-layer DDoS protection, serves a framework-aware SPA shim while remotes load.

Tenant-facing overview: [nexus.bimo.dk — Infrastructure: Gateway](https://nexus.bimo.dk/infrastructure/infra-gateway).
Contributor docs: [Internals — architecture](https://nexus.bimo.dk/internals/nexus-gateway/architecture) · [code map](https://nexus.bimo.dk/internals/nexus-gateway/code-map).

## Build and run

```bash
# Build
cargo build --release

# Run (requires a reachable registry)
NEXUS_TOKEN=dev-token \
REGISTRY_URL=http://localhost:8670 \
NEXUS_GATE_NAME=shop.example.com \
  ./target/release/nexus-gateway
```

The full local stack is in `nexus-test/` — `pwsh ./start.ps1` from there brings up the registry, gateway, and a tenant host on Docker.

## Env vars (required)

| Var | Purpose |
|---|---|
| `NEXUS_TOKEN` | Shared secret for registry HTTP + WS. |
| `REGISTRY_URL` | e.g. `http://registry:8670`. |
| `NEXUS_GATE_NAME` | The gate domain this instance serves (`shop.example.com`). |

Optional auto-registration: `NEXUS_HOST_NAME`, `NEXUS_HOST_URL`, `NEXUS_HOST_FRAMEWORK`, `NEXUS_HOST_REMOTE_ENTRY`, `NEXUS_HOST_EXPOSED_MODULE`, `NEXUS_GATE_LABEL`. Full list: [reference/environment](https://nexus.bimo.dk/reference/environment).

## Test

```bash
cargo test
```

`wiremock`-backed integration tests cover bootstrap, the seven protection layers, route table semantics, SPA injection escaping, and headers policy.
