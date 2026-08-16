#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
flags_file="${1:-$repo_root/docs/release/feature-flags.json}"

node - "$flags_file" <<'NODE'
const fs = require("node:fs");
const path = process.argv[2];
const document = JSON.parse(fs.readFileSync(path, "utf8"));
const today = new Date(`${process.env.FLAG_VALIDATION_DATE ?? new Date().toISOString().slice(0, 10)}T00:00:00Z`);

if (document.version !== 1 || !Array.isArray(document.flags) || document.flags.length === 0) {
  throw new Error("feature flag registry must declare version 1 and at least one flag");
}

const names = new Set();
for (const flag of document.flags) {
  if (!/^[a-z][a-z0-9_]{2,63}$/.test(flag.name) || names.has(flag.name)) {
    throw new Error(`invalid or duplicate flag name: ${flag.name}`);
  }
  names.add(flag.name);
  if (flag.default !== false) throw new Error(`${flag.name} must be default-off`);
  if (!/^[a-z][a-z0-9-]{2,31}$/.test(flag.owner)) {
    throw new Error(`${flag.name} must have a stable owner slug`);
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(flag.removal_date)) {
    throw new Error(`${flag.name} has an invalid removal date`);
  }
  const removalDate = new Date(`${flag.removal_date}T00:00:00Z`);
  if (!Number.isFinite(removalDate.valueOf()) || removalDate <= today) {
    throw new Error(`${flag.name} removal date must be in the future`);
  }
  if (typeof flag.description !== "string" || flag.description.trim() === "") {
    throw new Error(`${flag.name} must have a description`);
  }
}

console.log(`feature flags valid: ${document.flags.length}; all default=false; owners and future removal dates present`);
NODE
