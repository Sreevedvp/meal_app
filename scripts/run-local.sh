#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -f .env ]]; then set -a; source .env; set +a; fi
: "${DATABASE_URL:?Set DATABASE_URL, or copy .env.example to .env after starting PostgreSQL.}"
DX_BIN="${DX_BIN:-dx}"
if [[ "$DX_BIN" == dx && -x work/tools/dx ]]; then DX_BIN="$PWD/work/tools/dx"; fi
"$DX_BIN" build --platform web -p meal-app --release --debug-symbols false --locked
export WEB_DIST="${WEB_DIST:-target/dx/meal-app/release/web/public}"
exec cargo run -p meal-server --locked
