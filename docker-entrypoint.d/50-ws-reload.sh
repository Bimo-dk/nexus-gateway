#!/bin/sh
# Background daemon: polls registry every POLL_INTERVAL seconds and reloads
# nginx routes when remotes change. Forks immediately so nginx can start.
set -eu

REGISTRY_INTERNAL_URL="${REGISTRY_INTERNAL_URL:-http://registry:8670}"
NEXUS_TOKEN="${NEXUS_TOKEN:-}"
REMOTES_CONF=/etc/nginx/nexus-remotes.conf
POLL_INTERVAL="${NEXUS_RELOAD_INTERVAL:-15}"

fetch_remotes() {
  if [ -n "$NEXUS_TOKEN" ]; then
    wget -qO- --header="X-Nexus-Token: ${NEXUS_TOKEN}" \
      "${REGISTRY_INTERNAL_URL}/remotes" 2>/dev/null
  else
    wget -qO- "${REGISTRY_INTERNAL_URL}/remotes" 2>/dev/null
  fi
}

build_conf() {
  local response="$1"
  local tmpfile
  tmpfile=$(mktemp)

  printf '# remotes.conf — generated %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$tmpfile"

  while IFS=' ' read -r name upstream_url; do
    [ -z "$name" ] && continue
    printf '\nlocation ^~ /remotes/%s/ {\n  set $upstream_%s %s;\n  rewrite ^/remotes/%s(/.*)$ $1 break;\n  proxy_pass $upstream_%s;\n  proxy_set_header Host $host;\n  proxy_set_header X-Real-IP $remote_addr;\n  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n  proxy_set_header X-Forwarded-Proto $scheme;\n  proxy_http_version 1.1;\n}\n' \
      "$name" "$name" "$upstream_url" "$name" "$name" >> "$tmpfile"
  done <<PAIRS
$(printf '%s' "$response" | jq -r '.remotes[] | select(.upstreamUrl != null and .upstreamUrl != "") | "\(.name) \(.upstreamUrl)"' 2>/dev/null || true)
PAIRS

  printf '%s' "$tmpfile"
}

watch_loop() {
  sleep 5

  while true; do
    sleep "$POLL_INTERVAL"

    response=$(fetch_remotes) || response=""
    [ -z "$response" ] && continue

    tmpfile=$(build_conf "$response")
    [ -z "$tmpfile" ] && continue

    current_hash=$(tail -n +2 "$REMOTES_CONF" 2>/dev/null | md5sum | cut -d' ' -f1) || current_hash="none"
    new_hash=$(tail -n +2 "$tmpfile" | md5sum | cut -d' ' -f1)

    if [ "$current_hash" != "$new_hash" ]; then
      mv "$tmpfile" "$REMOTES_CONF"
      echo "[ws-reload] Remote routes changed — reloading nginx" >&2
      nginx -s reload 2>/dev/null || true
    else
      rm -f "$tmpfile"
    fi
  done
}

watch_loop &
