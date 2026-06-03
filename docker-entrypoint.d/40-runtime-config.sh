#!/bin/sh
# Genererer /usr/share/nginx/html/assets/config.json fra container env-vars.
# Køres af nginx:alpine's standard entrypoint før nginx starter.
#
# Tilføj nye runtime-configurable felter til assets/config.template.json
# og eksporter dem her med deres default-værdier.
set -eu

TEMPLATE=/usr/share/nginx/html/assets/config.template.json
OUTPUT=/usr/share/nginx/html/assets/config.json

if [ ! -f "$TEMPLATE" ]; then
  echo "[runtime-config] Template not found: $TEMPLATE — skipping" >&2
  exit 0
fi

export HOST_REMOTE_ENTRY="${HOST_REMOTE_ENTRY:-/host/remoteEntry.json}"
export HOST_EXPOSED_MODULE="${HOST_EXPOSED_MODULE:-./AppShell}"

envsubst < "$TEMPLATE" > "$OUTPUT"

echo "[runtime-config] Generated $OUTPUT with:"
echo "  HOST_REMOTE_ENTRY=$HOST_REMOTE_ENTRY"
echo "  HOST_EXPOSED_MODULE=$HOST_EXPOSED_MODULE"
