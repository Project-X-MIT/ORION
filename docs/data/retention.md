# Retention and account lifecycle

Active data is retained while an account is active. An account deletion request
immediately revokes sessions, anonymizes direct identifiers and removes
disposable cache entries. Immutable audit and rating-ledger records retain only
the minimum event identity needed for fraud, reconciliation and legal duties;
they are not editable by ordinary application roles. Backups follow the
approved encrypted retention schedule and are not used to reintroduce deleted
active identifiers after restore.
