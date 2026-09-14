# Validation — 14 September 2026

Verified locally on Apple Silicon macOS with Rust 1.96.0, Dioxus 0.7.10, PostgreSQL 18.4 and Playwright 1.62.1/Chromium.

| Check | Result |
| --- | --- |
| Domain and API unit tests | 31 passed |
| Real PostgreSQL integration tests | 5 passed, including production ownership, CSRF, quotas and revision conflicts |
| Rust formatting | Passed |
| Domain/server Clippy, all targets, warnings denied | Passed |
| Web client Clippy, warnings denied | Passed |
| Development web release build | Passed |
| Production `MEAL_REQUIRE_AUTH=true` web build | Passed |
| Nutrition/food-library browser checks | Passed, including search, pagination, varied meals, target periods, budget and offline recovery |
| Calendar browser checks | Passed, including date popups, keyboard focus, swaps, online browsing, saves/restores, shopping and mobile layouts |
| Plan-management browser checks | Passed, including same-ID edits, copying, deletion and failed-request retries |
| Account browser checks | Passed with mocked OIDC session responses; account-prefixed preferences, sign-out clearing and expired-session rejection |
| Android debug packaging | Passed; APK generated and kept outside Git |
| Production Compose interpolation/schema | Passed with placeholder credentials and `config --quiet` |
| Secret generator | Passed in a temporary fixture: mode 0600 and no overwrite |
| Repository hygiene | No eligible files over 5 MB; secret files, tools, databases, backups and generated artifacts ignored |
| npm dependency audit | Zero known vulnerabilities reported |
| Rust advisory gate | Passed with a conditional unused-RSA exception; five upstream warnings remain, detailed in [DEPENDENCIES.md](DEPENDENCIES.md) |

The development server was restarted with the updated API and the final development web build remains available at `http://127.0.0.1:8081` while its process is running. No commit, GitHub repository, remote push or public deployment was made.

## Not yet verified

Docker Engine was unavailable on this machine, so the production image was not built or run locally. Caddy and oauth2-proxy runtime configuration, public TLS issuance, real provider login, restart/restore drills, image scanning, load testing and live deployment require the deployment acceptance checks. CI workflows have been added but have not run on GitHub yet. Android installation, physical-device behavior, release signing and native account integration remain unverified.

All nutrition, allergen and price fixtures remain unverified demo data. Passing these tests verifies implementation against those fixtures, not real-world dietary suitability or a guaranteed grocery budget.
