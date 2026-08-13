#!/usr/bin/env bash
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL must point at the target database}"
export ORION_MIGRATE_ONLY=1
cargo run --locked --release -p orion-worker
