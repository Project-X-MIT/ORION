#!/usr/bin/env bash
set -euo pipefail

: "${TARGET_DATABASE_URL:?TARGET_DATABASE_URL is required}"
: "${BACKUP_ENCRYPTION_KEY:?BACKUP_ENCRYPTION_KEY is required}"
: "${RESTORE_ISOLATED:?set RESTORE_ISOLATED=1 for an isolated drill}"
if [[ "$RESTORE_ISOLATED" != 1 ]]; then
  echo 'refusing restore without RESTORE_ISOLATED=1' >&2
  exit 1
fi
backup_path="${1:?usage: restore.sh BACKUP.dump.enc}"
[[ -f "$backup_path" ]] || { echo "backup not found: $backup_path" >&2; exit 1; }
plain="$(mktemp "${TMPDIR:-/tmp}/orion-restore.XXXXXX.dump")"
trap 'rm -f "$plain"' EXIT
openssl enc -d -aes-256-cbc -pbkdf2 -in "$backup_path" -out "$plain" -pass env:BACKUP_ENCRYPTION_KEY
pg_restore --exit-on-error --no-owner --no-privileges --dbname "$TARGET_DATABASE_URL" "$plain"
printf 'restored isolated database from %s\n' "$backup_path"
