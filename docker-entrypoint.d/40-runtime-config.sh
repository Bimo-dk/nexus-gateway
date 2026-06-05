#!/bin/sh
# Genererer /usr/share/nginx/html/assets/config.json fra container env-vars.
# Genererer /etc/nginx/conf.d/remotes.conf fra registry API.
# Køres af nginx:alpine's standard entrypoint før nginx starter.
set -eu

TEMPLATE=/usr/share/nginx/html/assets/config.template.json
OUTPUT=/usr/share/nginx/html/assets/config.json
REMOTES_CONF=/etc/nginx/nexus-remotes.conf

if [ ! -f "$TEMPLATE" ]; then
  echo "[runtime-config] Template not found: $TEMPLATE — skipping" >&2
  exit 0
fi

export HOST_REMOTE_ENTRY="${HOST_REMOTE_ENTRY:-/host/remoteEntry.json}"
export HOST_EXPOSED_MODULE="${HOST_EXPOSED_MODULE:-./AppShell}"
export NEXUS_TOKEN="${NEXUS_TOKEN:-}"

envsubst < "$TEMPLATE" > "$OUTPUT"

echo "[runtime-config] Generated $OUTPUT with:"
echo "  HOST_REMOTE_ENTRY=$HOST_REMOTE_ENTRY"
echo "  HOST_EXPOSED_MODULE=$HOST_EXPOSED_MODULE"
echo "  NEXUS_TOKEN=<redacted>"

# Generate /etc/nginx/conf.d/remotes.conf from registry API.
# On failure (registry not yet ready) writes an empty placeholder so nginx
# can start; 50-ws-reload.sh will populate routes once the registry is up.
REGISTRY_INTERNAL_URL="${REGISTRY_INTERNAL_URL:-http://registry:8670}"
NEXUS_TOKEN="${NEXUS_TOKEN:-}"

fetch_remotes() {
  if [ -n "$NEXUS_TOKEN" ]; then
    wget -qO- --header="X-Nexus-Token: ${NEXUS_TOKEN}" \
      "${REGISTRY_INTERNAL_URL}/remotes" 2>/dev/null
  else
    wget -qO- "${REGISTRY_INTERNAL_URL}/remotes" 2>/dev/null
  fi
}

write_remote_conf() {
  local response="$1"
  local dest="$2"
  local tmpfile
  tmpfile=$(mktemp)

  printf '# remotes.conf — generated %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$tmpfile"

  while IFS=' ' read -r name upstream_url; do
    [ -z "$name" ] && continue
    printf '\nlocation ^~ /remotes/%s/ {\n  set $upstream_%s %s;\n  rewrite ^/remotes/%s(/.*)$ $1 break;\n  proxy_pass $upstream_%s;\n  proxy_set_header Host $host;\n  proxy_set_header X-Real-IP $remote_addr;\n  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n  proxy_set_header X-Forwarded-Proto $scheme;\n  proxy_http_version 1.1;\n}\n' \
      "$name" "$name" "$upstream_url" "$name" "$name" >> "$tmpfile"
    echo "[remotes-conf] Added proxy: $name → $upstream_url" >&2
  done <<PAIRS
$(printf '%s' "$response" | jq -r '.remotes[] | select(.upstreamUrl != null and .upstreamUrl != "") | "\(.name) \(.upstreamUrl)"' 2>/dev/null || true)
PAIRS

  mv "$tmpfile" "$dest"
}

response=$(fetch_remotes) || response=""

if [ -z "$response" ]; then
  echo "[remotes-conf] Registry not reachable at startup — writing empty remotes.conf" >&2
  printf '# remotes.conf — populated by 50-ws-reload.sh once registry is ready\n' > "$REMOTES_CONF"
else
  write_remote_conf "$response" "$REMOTES_CONF"
fi
