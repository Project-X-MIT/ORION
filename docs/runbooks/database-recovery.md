# Database recovery and backup monitoring

Backups are custom-format PostgreSQL dumps encrypted before leaving the
backup directory. The backup script writes a SHA-256 sidecar and success or
failure marker. The monitoring profile exposes backup age, failure and latest
restore-test status to Prometheus; `OrionBackupStale`, `OrionBackupFailure` and
`OrionRestoreTestMissing` route to the normal Alertmanager receiver.

For a complete synthetic drill, create a new empty database and run:

```bash
DATABASE_URL=postgres://.../orion \
TARGET_DATABASE_URL=postgres://.../orion_recovery_20260816 \
BACKUP_ENCRYPTION_KEY="$BACKUP_ENCRYPTION_KEY" \
RESTORE_ISOLATED=1 DRILL_OPERATOR="$USER" \
RPO_SECONDS=86400 RTO_SECONDS=3600 \
DRILL_REPORT=.staging-evidence/recovery-drill.txt \
scripts/recovery-drill.sh
```

The drill verifies the checksum, decrypts, restores with `--exit-on-error`,
compares every public-table row count and primary/unique/foreign/check
constraint count, checks foreign-key validation, and reconciles the rating
ledger/current ratings, outbox, user/research/quiz/course/news/notification
lifecycle invariants. It records the operator, UTC timestamps, restore point,
RPO/RTO measurements and follow-up. Drop the isolated database only after the
report has been reviewed; never restore over production.

Record operator, UTC start/end, backup object/checksum, restore point, RPO/RTO,
validation results and follow-ups. A corrupt migration is recovered by
restoring the last valid point into isolation, applying a forward compensating
migration (never editing an applied migration), validating it with the drill,
and rehearsing again before cutover. Accidental deletion uses the same point-
in-time isolated restore; export only the required rows or promote the
isolated database after approval. Credential loss pauses backup jobs, rotates
the database and encryption credentials in the secret manager, verifies a new
encrypted backup and restores a fresh isolated copy before cutover. Revoke the
old credentials and record the rotation timestamp. Redis is rebuilt from
PostgreSQL and is never an authoritative recovery source.
