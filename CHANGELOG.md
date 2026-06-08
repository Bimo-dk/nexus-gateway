# nexus-gateway

## 1.0.0

First stable release. The gateway is now feature-complete for the 1.0
platform release and follows semver from here on.

### Highlights

- Stateless Rust + axum + hyper public ingress. Listens on `:8668`,
  proxies to host shells and remote micro frontends.
- Reads its routing table from the registry over WebSocket. Hot-swap of
  hosts, gates, and remotes lands in the routing table in milliseconds
  with no restart.
- Multi-domain gate routing. One gateway can serve several public
  domains, each mapped to a different host shell.
- Seven-layer DDoS protection: per-IP rate limit, body size cap,
  header storm guard, TLS hand-shake throttle, WebSocket frame
  throttle, slow-loris timeout, ban list. Settings are
  hot-configurable from the portal.
- Framework-aware SPA shim with cache headers tuned per-remote.
- Built-in Prometheus metrics on `/metrics`.

### Fixes

- (B-22) Registry-listener route updates now use `remote.upstream_url`
  (internal upstream like `http://remote-x:80`) instead of `remote.url`
  (the browser-facing manifest path). Previously WS-driven route
  changes after startup overwrote the correct startup-time target with
  the registry's url field, producing 502 errors with concatenated
  paths like `/remotes/x/remoteEntry.json/remoteEntry.json`. Skips
  remotes whose `upstreamUrl` is empty and logs both fields.

### License

Relicensed from MIT to GNU Affero General Public License v3.0 or any
later version (AGPL-3.0-or-later). Commercial license: svp@bimo.dk.
