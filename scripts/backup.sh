#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${BACKUP_ENCRYPTION_KEY:?BACKUP_ENCRYPTION_KEY is required}"
backup_dir="${BACKUP_DIR:-${PWD}/.orion-backups}"
mkdir -p "$backup_dir"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
plain="$(mktemp "${TMPDIR:-/tmp}/orion-backup.XXXXXX.dump")"
encrypted="$backup_dir/orion-${timestamp}.dump.enc"
trap 'rm -f "$plain"' EXIT

pg_dump --format=custom --no-owner --no-privileges "$DATABASE_URL" > "$plain"
pg_restore --list "$plain" >/dev/null
openssl enc -aes-256-cbc -pbkdf2 -salt -in "$plain" -out "$encrypted" -pass env:BACKUP_ENCRYPTION_KEY
sha256sum "$encrypted" > "$encrypted.sha256"
chmod 600 "$encrypted" "$encrypted.sha256"
printf 'created encrypted backup %s\n' "$encrypted"
