#!/usr/bin/env bash
set -euo pipefail
: "${APP_DB_PASSWORD:?Set APP_DB_PASSWORD}"
psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" --set=ON_ERROR_STOP=1 --set=app_password="$APP_DB_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE meal_app LOGIN PASSWORD %L', :'app_password') \gexec
GRANT CONNECT ON DATABASE meal_planner TO meal_app;
GRANT USAGE ON SCHEMA public TO meal_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO meal_app;
SQL
