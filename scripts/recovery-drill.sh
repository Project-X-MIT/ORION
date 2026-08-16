#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${TARGET_DATABASE_URL:?TARGET_DATABASE_URL is required}"
: "${BACKUP_ENCRYPTION_KEY:?BACKUP_ENCRYPTION_KEY is required}"
: "${RESTORE_ISOLATED:?set RESTORE_ISOLATED=1 for an isolated drill}"
: "${DRILL_OPERATOR:?DRILL_OPERATOR identifies the recovery operator}"
[[ "$RESTORE_ISOLATED" == 1 ]] || { echo 'refusing recovery drill without RESTORE_ISOLATED=1' >&2; exit 1; }

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
backup_dir="${BACKUP_DIR:-$repo_root/.orion-backups}"
report_file="${DRILL_REPORT:-$repo_root/.staging-evidence/recovery-drill-$(date -u +%Y%m%dT%H%M%SZ).txt}"
rpo_seconds="${RPO_SECONDS:-86400}"
rto_seconds="${RTO_SECONDS:-3600}"
started_epoch="$(date -u +%s)"
mkdir -p "$(dirname "$report_file")" "$backup_dir"
chmod 700 "$backup_dir"
exec > >(tee "$report_file") 2>&1

source_psql() { psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -Atc "$1"; }
target_psql() { psql "$TARGET_DATABASE_URL" -X -v ON_ERROR_STOP=1 -Atc "$1"; }
assert_zero() {
  local label="$1" query="$2" source_value target_value
  source_value="$(source_psql "$query")"
  target_value="$(target_psql "$query")"
  printf '%s source=%s target=%s\n' "$label" "$source_value" "$target_value"
  [[ "$source_value" == 0 && "$target_value" == 0 ]] || { echo "invariant failed: $label" >&2; exit 1; }
}

echo "operator=$DRILL_OPERATOR"
echo "started_epoch=$started_epoch"
echo "rpo_seconds=$rpo_seconds rto_seconds=$rto_seconds"
echo "restore_isolated=$RESTORE_ISOLATED"
DATABASE_URL="$DATABASE_URL" BACKUP_ENCRYPTION_KEY="$BACKUP_ENCRYPTION_KEY" BACKUP_DIR="$backup_dir" "$repo_root/scripts/backup.sh"
backup_path=""
for candidate in "$backup_dir"/*.dump.enc; do
  [[ -f "$candidate" ]] && backup_path="$candidate"
done
[[ -n "$backup_path" ]] || { echo 'encrypted backup was not created' >&2; exit 1; }
backup_epoch="$(stat -c %Y "$backup_path" 2>/dev/null || stat -f %m "$backup_path")"
backup_age=$((started_epoch - backup_epoch)); (( backup_age < 0 )) && backup_age=0
echo "backup_path=$backup_path"
echo "backup_sha256=$(awk '{print $1}' "$backup_path.sha256")"
echo "backup_age_seconds=$backup_age"
(( backup_age <= rpo_seconds )) || { echo 'RPO exceeded before restore' >&2; exit 1; }

restore_started_epoch="$(date -u +%s)"
TARGET_DATABASE_URL="$TARGET_DATABASE_URL" BACKUP_ENCRYPTION_KEY="$BACKUP_ENCRYPTION_KEY" RESTORE_ISOLATED=1 "$repo_root/scripts/restore.sh" "$backup_path"
restore_duration=$(( $(date -u +%s) - restore_started_epoch ))
echo "restore_duration_seconds=$restore_duration"
(( restore_duration <= rto_seconds )) || { echo 'RTO exceeded during restore' >&2; exit 1; }

source_counts="$(mktemp)"; target_counts="$(mktemp)"
trap 'rm -f "$source_counts" "$target_counts"' EXIT
tables="$(source_psql "select table_name from information_schema.tables where table_schema='public' and table_type='BASE TABLE' order by table_name")"
while IFS= read -r table; do
  [[ -n "$table" ]] || continue
  source_count="$(source_psql "select count(*) from \"$table\"")"
  target_count="$(target_psql "select count(*) from \"$table\"")"
  printf '%s\t%s\n' "$table" "$source_count" >> "$source_counts"
  printf '%s\t%s\n' "$table" "$target_count" >> "$target_counts"
done <<< "$tables"
echo 'row_counts_source:'; cat "$source_counts"
echo 'row_counts_target:'; cat "$target_counts"
diff -u "$source_counts" "$target_counts"

for kind in p u f c; do
  source_constraints="$(source_psql "select count(*) from pg_constraint where contype='$kind'")"
  target_constraints="$(target_psql "select count(*) from pg_constraint where contype='$kind'")"
  echo "constraints_${kind}_source=$source_constraints target=$target_constraints"
  [[ "$source_constraints" == "$target_constraints" ]] || { echo "constraint count mismatch: $kind" >&2; exit 1; }
done
assert_zero 'unvalidated foreign keys' "select count(*) from pg_constraint where contype='f' and not convalidated"
assert_zero 'rating ledger arithmetic/range' "select count(*) from rating_ledger where rating_delta <> rating_after - rating_before or rating_before not between 1 and 4000 or rating_after not between 1 and 4000"
assert_zero 'current rating parity' "select count(*) from user_ratings ur where rating not between 1 and 4000 or games_played <> wins + losses + draws or rating <> coalesce((select rl.rating_after from rating_ledger rl where rl.user_id = ur.user_id order by rl.created_at desc, rl.id desc limit 1), 1200)"
assert_zero 'users lifecycle' "select count(*) from users where not ((status='active' and disabled_at is null and deleted_at is null) or (status='disabled' and disabled_at is not null and deleted_at is null) or (status='deleted' and deleted_at is not null))"
assert_zero 'research lifecycle' "select count(*) from research_papers where (status in ('submitted','under_review','approved','rejected','published') and submitted_at is null) or (status in ('under_review','approved','rejected','published') and under_review_at is null) or (status in ('approved','rejected','published') and (decided_by is null or decided_at is null)) or (status='published' and published_at is null) or ((not elo_awarded) and (elo_award is not null or elo_awarded_at is not null)) or (elo_awarded and (elo_award is null or elo_awarded_at is null))"
assert_zero 'course progress lifecycle' "select count(*) from course_progress where (completed and completed_at is null) or ((not completed) and completed_at is not null)"
assert_zero 'quiz attempt lifecycle' "select count(*) from quiz_attempts where (status='pending' and completed_at is not null) or (status='completed' and completed_at is null)"
assert_zero 'news ingestion lifecycle' "select count(*) from news_ingestion_runs where (status <> 'running' and completed_at is null) or articles_seen < 0 or articles_inserted < 0"
assert_zero 'notification lifecycle' "select count(*) from notifications where expires_at is not null and expires_at <= created_at"
assert_zero 'outbox lifecycle' "select count(*) from outbox_events where retry_count < 0 or job_attempts < 0 or schema_version <= 0"

echo "completed_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'status=pass\noperator=%s\nrestore_point=%s\n' "$DRILL_OPERATOR" "$backup_path" > "$backup_dir/.restore-test-success"
chmod 600 "$backup_dir/.restore-test-success"
echo 'follow_up=review report, retain encrypted object per policy, and approve cutover separately'
echo "recovery drill PASS report=$report_file"
