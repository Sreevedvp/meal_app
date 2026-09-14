# Architecture

`crates/domain` owns the catalogue contracts, nutrition calculation, full-pack pricing, exclusions, calendar arithmetic and bounded meal search. Dioxus (`crates/app`) runs the same rules for interactive planning. Axum (`crates/server`) independently recalculates submitted meal slots before storing them with SQLx in PostgreSQL.

PostgreSQL stores normalized, indexed food records and immutable catalogue snapshots. Each saved calendar references its original snapshot, so editing a historical plan does not silently change prices or ingredients. Plan reads, writes, listings and deletes are restricted by `owner_id`. Updates use optimistic concurrency: the submitted `revision` must match; successful writes increment it. DELETE requires the saved revision in `If-Match`. A stale request returns 409 and leaves the draft intact. The list API returns 20 summaries per page. An advisory transaction lock serializes creates for each user's storage quota.

## Identity boundary

Production traffic follows **HTTPS Caddy → oauth2-proxy/OIDC → private Caddy listener → private Axum API → private PostgreSQL**. Only ports 80/443 are published. Public Caddy strips client identity headers. The internal listener accepts only the proxy's fixed network address and inserts an independent secret. Axum verifies this secret and hashes the issuer namespace plus provider user ID into the owner key. Changing the issuer namespace changes ownership; plan migration must be explicit.

The API also validates Origin on production mutations, applies per-user request limits, bounds request bodies and concurrency, limits SQL/HTTP execution time, and returns generic database errors. Logs contain request IDs, route patterns, status and duration. API responses and HTML are not cached. CSP restricts scripts and connections to the app origin; `unsafe-eval` remains necessary for Dioxus `document::eval` (dialog focus and browser storage). This is a documented exception, not a claim of a strict nonce-only CSP. Multi-instance deployments need a shared limiter; the supplied deployment runs one API process.

Production builds require `/api/session` before loading account-prefixed browser storage. Sign out clears the current account's preferences and recent calendars on that device before clearing the proxy cookie. Identity-provider single sign-on may remain active. Other devices' saved progress is not synchronized. Browser local storage is not encrypted; avoid retaining plans on shared devices. The backend does not store meal-completion or shopping checks.

Development mode is loopback-only and uses the reserved `development` owner, preserving earlier local plans. Legacy price-only endpoints remain development-only. Production native Android login is not implemented; the native client defaults to offline use.

## Limits

The 163 recipes and 93 ingredient records are authored fixtures, with 162 usable recipes. The optimizer samples bounded candidates; its cheapest result is not a proven global minimum. It enforces per-meal protein, calorie tolerance, exclusions, seven-day variety and the selected budget policy. It does not assess overall dietary adequacy. Grocery costs omit fuel, delivery, spoilage and pantry carryover between plans.

Eight recent snapshots are retained per browser account. Online calendars can be browsed and reopened, then edited or deleted. A response lost after a successful create can result in a duplicate on retry; inspect the online list before retrying an uncertain save. No automated account deletion, data export, multi-region operation or telemetry service is supplied. `crates/typing` and `crates/domain/src/typing.rs` are pre-existing, unwired extraction work outside the compiled workspace and are preserved as-is.
