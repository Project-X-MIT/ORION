#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec docker compose -f infra/compose/docker-compose.yml up --build
