# Deploying Deccan Table

The supplied deployment is a single-host HTTPS web application with PostgreSQL, Caddy and an OIDC login proxy. It requires a Linux host with Docker Engine/Compose, a domain pointing to that host, open ports 80/443, and an OIDC provider supporting authorization code flow with PKCE. It is not a managed hosting service.

**Release status:** container configuration is supplied, but local container execution could not be verified because Docker Engine was unavailable. Run the container CI job and the acceptance checks below before using this stack. Real identity-provider login and certificate issuance need your credentials/domain and have not been exercised. The fixture catalogue is not suitable for a verified nutrition product launch.

## Configure

```sh
python3 scripts/init-production-env.py
```

This creates a new ignored `.env.production` with permission 0600 and independently generated secrets; it refuses to overwrite an existing file. Fill in `PUBLIC_HOST`, `TLS_EMAIL`, your HTTPS issuer, client ID/secret, and allowed email domains. Register `https://YOUR_HOST/oauth2/callback` at the identity provider. Use a narrowly scoped email-domain allowlist; `*` intentionally admits all eligible provider accounts. Review that decision before changing it.

Keep generated database passwords URL-safe because they appear inside connection URLs. Store the environment file and encrypted backups in your secret manager. Do not paste Compose's resolved configuration into public logs; it contains secrets.

The application refuses production startup with the current fixture catalogue unless `ALLOW_DEMO_DATA=true` is explicitly set. This enables a visibly labelled **demo deployment** only. For a nutrition product launch, acquire reviewed nutrition/allergen/price data, implement and review the import path, version the catalogue and revise that gate as part of the reviewed data release. Setting the flag does not verify data.

```sh
docker compose --env-file .env.production -f compose.production.yaml config --quiet
docker compose --env-file .env.production -f compose.production.yaml build
docker compose --env-file .env.production -f compose.production.yaml up -d --wait
```

Only Caddy is published. Never publish the API, database, proxy or internal Caddy port. Keep the edge subnet and the two trusted fixed addresses consistent across Compose, Caddy and oauth2-proxy. Change all three configurations together if `172.30.0.0/24` overlaps your network. The application runs as UID 10001 with a read-only filesystem, bounded memory/CPU and no Linux capabilities. PostgreSQL uses a separate DML-only application role; a one-shot administrative service applies migrations first.

## Acceptance checks

- Confirm HTTPS with a valid certificate and rejection of unauthenticated `/api/session` requests.
- Sign in with two different provider accounts. Create a plan as one; the other must not list, load, change or delete it, even with the plan ID.
- Confirm calories/protein/budget/allergens are requested before generation, date popups work, and saved calendars can be browsed, edited, copied and deleted.
- Edit the same plan in two browser sessions. The stale save/delete must report a conflict. Reload the online version to continue.
- Verify sign out clears account-local preferences/calendars and that signing in as the other account shows its own data.
- Restart the application and database without deleting volumes; saved plans must remain. Restore a backup into an isolated environment before launch.
- Check provider admission rules, resource usage, disk usage, log retention and backup alerts. Run the GitHub checks; no failed check should be bypassed without investigation.

## Migrations, upgrades and rollback

Back up before each release. Tag the image with a reviewed Git commit using `APP_IMAGE_TAG`; retain the previous image. Build and test the new image, run the `migrate` service explicitly, then recreate the app. Migrations are embedded in the server and recorded by SQLx. Never modify an applied migration. Application rollback is safe only while the migrated schema remains backward-compatible; otherwise restore the backup in a maintenance window. There are no automatic down-migrations.

Migration 0004 assigns old prototype records to the reserved development owner. They are intentionally not granted to the first user who signs in. A reviewed, owner-specific data migration is required if these records should become production account data.

## Backups

These commands create a sensitive PostgreSQL custom-format backup. Choose an encrypted, access-controlled backup directory and set restrictive permissions first:

```sh
umask 077
mkdir -p .local/backups
docker compose --env-file .env.production -f compose.production.yaml exec -T db pg_dump -U meal_admin -d meal_planner -Fc > .local/backups/meal-planner.dump
```

Copy backups to encrypted off-host storage on a schedule, monitor failures, and define retention and recovery objectives. The command alone is not a backup service. Restore into an **empty isolated test database** using `pg_restore --exit-on-error --no-owner --no-acl`; use the target administrator and apply the application role grants. Do not test restoration over the live database. Database role passwords are not included in this dump; keep deployment secrets separately. Caddy volumes contain TLS state and should also be protected.

## Operational limits

The process exposes `/api/live` and `/api/ready` on the private network. Health checks expose status only. Production defaults to 120 requests/minute per account and 100 saved calendars/account; requests have a 32 KiB JSON limit, 30-second deadline and 16 concurrent API slots. One host is a single point of failure. Browser completion/shopping marks remain on the device. Android still needs native authentication design, release signing and physical-device testing before distribution as a production app.

References: [oauth2-proxy configuration](https://oauth2-proxy.github.io/oauth2-proxy/configuration/overview/), [Caddy reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy), [PostgreSQL backup documentation](https://www.postgresql.org/docs/18/backup.html).
