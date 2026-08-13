# Database recovery drill

Backups are custom-format PostgreSQL dumps encrypted before leaving the
backup directory. The operator exports `TARGET_DATABASE_URL` pointing to a
new isolated database and must set `RESTORE_ISOLATED=1`; the restore script
refuses to run otherwise. Verify the checksum, decrypt, restore with
`--exit-on-error`, apply forward migrations, and reconcile row counts, foreign
keys, the rating ledger/current ratings, outbox states and sampled lifecycles.

Record operator, UTC start/end, backup object/checksum, restore point, RPO/RTO,
validation results and follow-ups. A corrupt migration is recovered by
restoring the last valid point into isolation, fixing forward with a new
migration, and rehearsing again. Accidental deletion uses the same isolated
restore; credential loss rotates the encryption/database credentials before
cutover. Never restore over production during a drill.
