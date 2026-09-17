#!/usr/bin/env bash
# Update a self-built linXiv node (see raspberry-pi.md): snapshot the DB,
# rebuild the image, restart the unit, and confirm the node comes back.
#
# Snapshot first because the schema is pre-1.0 and the new image migrates on
# its next boot. The backup goes through the running node's own connection
# (POST /api/storage/backup vacuums to a temp file, then renames) rather than
# a second process on the same DB — the library is single-writer.
set -euo pipefail

REPO="${LINXIV_REPO:-$HOME/linXiv}"
ENV_FILE="${LINXIV_ENV_FILE:-$HOME/.config/linxiv/node.env}"
DATA="${LINXIV_DATA:-/mnt/linxiv-ssd/data}"
ADDR="${LINXIV_ADDR:-http://127.0.0.1:8000}"

# shellcheck source=/dev/null
. "$ENV_FILE"
: "${LINXIV_API_TOKEN:?no LINXIV_API_TOKEN in $ENV_FILE}"
auth=(-H "Authorization: Bearer $LINXIV_API_TOKEN")

stamp=$(date +%F-%H%M%S)
mkdir -p "$DATA/backups"
echo "[update] snapshotting to backups/$stamp.db"
curl -sf -X POST "${auth[@]}" -H 'Content-Type: application/json' \
  -d "{\"dest_path\":\"/data/backups/$stamp.db\"}" "$ADDR/api/storage/backup"
echo

echo "[update] pulling and rebuilding"
git -C "$REPO" pull --recurse-submodules --ff-only
# Cache mounts in the Dockerfile keep cargo's registry and target/, so this
# recompiles the linXiv crates only, not the whole dependency graph.
podman build -t linxiv-headless:local "$REPO"

echo "[update] restarting"
systemctl --user restart linxiv.service

# The node is only really up once migrations finish and the router answers.
for _ in $(seq 1 60); do
  if curl -sf "${auth[@]}" "$ADDR/api/status" >/dev/null; then
    echo "[update] healthy on $(git -C "$REPO" describe --tags --always)"
    podman image prune -f >/dev/null
    exit 0
  fi
  sleep 5
done

echo "[update] node did not come healthy in 5min. Logs:" >&2
journalctl --user -u linxiv.service -n 40 --no-pager >&2
echo "[update] to roll back: restore backups/$stamp.db via POST /api/storage/restore" >&2
exit 1
