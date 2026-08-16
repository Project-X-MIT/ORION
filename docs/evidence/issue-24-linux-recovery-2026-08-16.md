# DIV-15 backup, restore and disaster-recovery evidence

Run date: 2026-08-16  
Operator: `ShauryaBijalwan`  
Commit: `3fe6799`  
Environment: disposable synthetic Compose stack on `orion-vm` (Ubuntu),
`orion-local`; no production data or credentials.

## Recovery drill

The drill ran `scripts/recovery-drill.sh` with
`RESTORE_ISOLATED=1`, `RPO_SECONDS=86400` and `RTO_SECONDS=3600`, using a
new database named `orion_recovery_issue24`. The isolated database was dropped
after review.

- Encrypted backup: `orion-20260816T171341Z.dump.enc`
- SHA-256: `3ac614ad330f23849fadfc4941da7b0698905866a74494fa7d03b6b051c16e5d`
- Backup age at drill start: 0 seconds (RPO passed)
- Restore duration: 1 second (RTO passed)
- Result: `recovery drill PASS`

All 21 public-table row counts matched, including `users=12042` and
`user_ratings=12042`. Constraint counts matched exactly: primary key 84,
unique 59, foreign key 20 and check 108; all foreign keys were validated.
Rating-ledger arithmetic/ranges and current-rating parity were zero on both
source and restore. User, research, course-progress, quiz-attempt,
news-ingestion, notification and outbox lifecycle invariant violations were
zero on both databases.

## Backup alert drill

The monitoring profile scraped `backup-exporter:9101` successfully. With an
empty backup volume, `OrionBackupStale` became firing after its one-minute
hold and `OrionRestoreTestMissing` fired immediately. A synthetic
`.last-failure` marker then caused `OrionBackupFailure` to fire. Copying the
verified encrypted backup, checksum and `.restore-test-success` marker into
the backup volume made all three alerts resolve; Prometheus reported no active
alerts afterward and the receiver logged the firing and resolved notifications.

## Reproduction

```bash
DATABASE_URL=postgres://.../orion \
TARGET_DATABASE_URL=postgres://.../orion_recovery_issue24 \
BACKUP_ENCRYPTION_KEY="$BACKUP_ENCRYPTION_KEY" \
RESTORE_ISOLATED=1 DRILL_OPERATOR="$USER" \
RPO_SECONDS=86400 RTO_SECONDS=3600 \
DRILL_REPORT=.staging-evidence/recovery-drill.txt \
scripts/recovery-drill.sh

docker compose --env-file .env \
  -f infra/compose/docker-compose.yml --profile monitoring up -d
```

The full drill report remains under the VM's ignored `.staging-evidence/`
directory. The versioned runbook documents corrupt-migration, accidental-
deletion and credential-loss recovery, including forward-only migration and
secret-rotation requirements. Production cutover and final RPO/RTO approval
remain separate release decisions.
