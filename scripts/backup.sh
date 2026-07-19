#!/usr/bin/env bash
set -euo pipefail

workspace="${PAPER_CODEX_WORKSPACE:-/var/lib/paper-codex/workspace}"
destination="${1:-/var/backups/paper-codex}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
archive="$destination/paper-codex-$timestamp.tar.zst"
temporary="$(mktemp -d)"
service_was_active=false

cleanup() {
  if [[ "$service_was_active" == true ]]; then
    systemctl start paper-codex.service
  fi
  rm -rf -- "$temporary"
}
trap cleanup EXIT

if [[ ! -d "$workspace" || "$workspace" == "/" ]]; then
  echo "Refusing to back up invalid workspace: $workspace" >&2
  exit 1
fi

install -d -m 0700 "$destination"
if systemctl is-active --quiet paper-codex.service; then
  service_was_active=true
  systemctl stop paper-codex.service
fi

tar --zstd -cf "$archive" \
  --exclude='.paper-wiki/cache' \
  --exclude='.paper-wiki/indexes' \
  --exclude='.paper-wiki/staging' \
  -C "$workspace" .

sha256sum "$archive" > "$archive.sha256"
echo "$archive"
