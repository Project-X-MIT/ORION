#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${BACKUP_ENCRYPTION_KEY:?BACKUP_ENCRYPTION_KEY is required}"
backup_dir="${BACKUP_DIR:-${PWD}/.orion-backups}"
mkdir -p "$backup_dir"
chmod 700 "$backup_dir"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
plain="$(mktemp "${TMPDIR:-/tmp}/orion-backup.XXXXXX.dump")"
encrypted="$backup_dir/orion-${timestamp}.dump.enc"

on_exit() {
  status=$?
  if [[ "$status" -ne 0 ]]; then
    date -u +%s > "$backup_dir/.last-failure"
    chmod 600 "$backup_dir/.last-failure"
  fi
  rm -f "$plain"
  exit "$status"
}
trap on_exit EXIT

pg_dump --format=custom --no-owner --no-privileges "$DATABASE_URL" > "$plain"
pg_restore --list "$plain" >/dev/null
openssl enc -aes-256-cbc -pbkdf2 -salt -in "$plain" -out "$encrypted" -pass env:BACKUP_ENCRYPTION_KEY
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$encrypted" > "$encrypted.sha256"
else
  shasum -a 256 "$encrypted" > "$encrypted.sha256"
fi
chmod 600 "$encrypted" "$encrypted.sha256"
date -u +%s > "$backup_dir/.last-success"
chmod 600 "$backup_dir/.last-success"
printf 'created encrypted backup %s\n' "$encrypted"
