#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test -p meal-domain -p meal-server --locked
if [[ -n "${DATABASE_URL:-}" ]]; then
  cargo test -p meal-server --test postgres_flow --locked -- --ignored
else
  echo 'PostgreSQL integration test skipped: set DATABASE_URL to a local test database.'
fi
