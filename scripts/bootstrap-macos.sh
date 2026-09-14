#!/usr/bin/env bash
# Optional workspace-local tools for an Apple Silicon Mac. No system installs.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || { echo 'This helper is for Apple Silicon macOS; use your normal Dioxus/PostgreSQL installation elsewhere.'; exit 1; }
mkdir -p work/tools work/postgres
if [[ ! -x work/tools/dx ]]; then
  curl --fail --location --retry 2 https://github.com/DioxusLabs/dioxus/releases/download/v0.7.10/dx-aarch64-apple-darwin.tar.gz -o work/tools/dx.tar.gz
  echo '342c92bc33dca60dadd725ed5e249923f049ae2e1e50b7477f4968d56695b425  work/tools/dx.tar.gz' | shasum -a 256 -c -
  tar -xzf work/tools/dx.tar.gz -C work/tools
fi
if [[ ! -x work/postgres/package/native/bin/postgres ]]; then
  curl --fail --location --retry 2 https://registry.npmjs.org/@embedded-postgres/darwin-arm64/-/darwin-arm64-18.4.0-beta.17.tgz -o work/postgres/archive.tgz
  echo '1f68ab0148346d99e045a54497f72fa22f487618c97ca70fa373441d1ab1a4d3  work/postgres/archive.tgz' | shasum -a 256 -c -
  tar -xzf work/postgres/archive.tgz -C work/postgres
  (cd work/postgres/package && node scripts/hydrate-symlinks.js)
fi
PG_BIN="$PWD/work/postgres/package/native/bin"
if [[ ! -f work/pgdata/PG_VERSION ]]; then
  "$PG_BIN/initdb" -D work/pgdata -U meal_dev --auth=trust --encoding=UTF8 --locale=C
fi
# This disposable database uses local trust authentication and only binds loopback.
if ! "$PG_BIN/pg_ctl" -D work/pgdata status >/dev/null 2>&1; then
  mkdir -p work/pgsocket
  chmod 700 work/pgsocket
  "$PG_BIN/pg_ctl" -D work/pgdata -l work/postgres.log -o "-h 127.0.0.1 -p 55432 -k $PWD/work/pgsocket" -t 120 start
fi
cat <<'MESSAGE'
Local tools and PostgreSQL are ready. Run:
  DATABASE_URL=postgres://meal_dev@127.0.0.1:55432/postgres ./scripts/run-local.sh
To stop PostgreSQL, preserving data:
  work/postgres/package/native/bin/pg_ctl -D work/pgdata stop
MESSAGE
