# Rating reconciliation

Compare the immutable `rating_ledger` aggregate with each current rating in an
isolated read-only transaction. Stop settlement if a mismatch is detected;
never rewrite ledger history. Record the affected user count, first event ID,
and UTC timestamps, then run the owning feature's forward repair transaction.
