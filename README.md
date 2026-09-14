# Deccan Table

Indian meal planning around calorie and protein targets, food exclusions and an INR grocery budget. Built with **Rust, Dioxus, Axum, SQLx and PostgreSQL** for the web, with an offline Android client.

**Pre-release:** the app works with 163 authored Indian dish fixtures and 93 ingredients. Nutrition and prices are unverified demo estimates; 162 dishes are usable and one is intentionally blocked for missing allergen metadata. The lowest sampled cost is not a proven minimum monthly budget. Review [release validation](docs/VALIDATION.md) and [deployment requirements](docs/DEPLOYMENT.md) before hosting it.

## Features

- Set calories and protein per meal, day, week or month; choose a monthly budget, diet priority, food preference and allergens upfront.
- Generate varied seven-day or monthly plans. Dishes have a seven-day repeat gap; impossible targets produce an explanation instead of a repetitive fallback.
- Use a real month calendar. Click a date for a keyboard-accessible popup with meals, portions, recipes, nutrition, completion marks and valid swaps.
- Edit or delete saved plans, copy a menu to new dates, and filter swaps to cheaper choices.
- Shop from weekly lists that carry unused ingredient packs forward and reconcile to the period's grocery cost.
- Browse the indexed Indian food library, save calendars in PostgreSQL, and browse or restore older online plans.
- Account-owned plans and version checks protect hosted plan access and prevent stale edits from overwriting newer changes. Production connection settings are not exposed in the UI.

The browser retains eight recent account-scoped calendars and local completion/shopping marks. Planning can use bundled data after assets have loaded; this is not an installable offline PWA. The server independently validates saved menus and keeps historical catalogue snapshots.

## Local setup

Requirements: Rust **1.96.0**, Dioxus CLI **0.7.10**, PostgreSQL **18**, and Node.js 22+ for optional browser tests. Development is restricted to loopback and uses one shared development owner.

```sh
rustup target add wasm32-unknown-unknown
./scripts/install-dioxus.sh
docker compose up -d --wait
cp .env.example .env
./scripts/run-local.sh
```

Open **http://127.0.0.1:8081**. The script builds the web client and starts the API; migrations and fixture seeding run automatically in development. Ctrl-C stops the API. Docker must be running and port 55432 available. Use a database dedicated to this project.

On Apple Silicon, the existing optional portable setup avoids Docker:

```sh
./scripts/bootstrap-macos.sh
DATABASE_URL=postgres://meal_dev@127.0.0.1:55432/postgres ./scripts/run-local.sh
```

Do not run both databases on the same port. The portable helper uses local trust authentication and downloads pinned tools into ignored `work/`; keep it private to your machine. Stop it with `work/postgres/package/native/bin/pg_ctl -D work/pgdata stop`. If `.env` exists, the run script loads it; make sure it matches the selected database.

## Checks

```sh
./scripts/check.sh
DATABASE_URL=postgres://meal_dev@127.0.0.1:55432/postgres ./scripts/check.sh
cargo clippy -p meal-domain -p meal-server --all-targets --locked -- -D warnings
```

With the built app running locally:

```sh
npm ci --ignore-scripts
npx playwright install chromium
npm run test:browser
```

Browser tests create fixture calendars and screenshots in ignored `outputs/`. `PLAYWRIGHT_MODULE` and `CHROMIUM_PATH` can select an existing installation. GitHub Actions runs unit, PostgreSQL, lint, web, browser and container build checks. The first remote run still needs to pass; adding a workflow does not verify it has run.

## Hosted deployment

Follow [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md). The supplied Compose stack uses HTTPS Caddy, OIDC via oauth2-proxy, private API/database networks, a restricted database role and explicit migrations. Generate local secrets using `python3 scripts/init-production-env.py`, then configure your own domain and identity provider. The production image requires authenticated session initialization before reading browser data.

The fixture catalogue blocks production startup by default. `ALLOW_DEMO_DATA=true` explicitly permits a labelled demo deployment; it does not make nutrition data verified. Container execution and a real provider login must be validated before launch. Android needs native account integration, release signing and device testing before store distribution.

## Android development

The same Rust client builds as an Android WebView application. Install Rust target `aarch64-linux-android`, JDK 17 and Android SDK/NDK; set `JAVA_HOME`, `ANDROID_HOME` and `ANDROID_NDK_HOME`, then run `./scripts/build-android.sh`. The tested NDK is 27.1.12297006. The debug APK is generated under `target/dx/meal-app/debug/android/app/app/build/outputs/apk/debug/` and is excluded from Git.

Android defaults to offline use. `MEAL_API_URL` is a developer-only compile-time override. Local API testing can use `adb reverse tcp:8081 tcp:8081`; production cookie login is a web deployment flow, not native authentication support.

## Repository

| Path | Purpose |
| --- | --- |
| `crates/domain` | Shared calculations, rules, fixtures and tests |
| `crates/server` | API, account isolation, migrations and database tests |
| `crates/app` | Dioxus client and responsive styles |
| `deploy`, `compose.production.yaml`, `Dockerfile` | Hosted deployment configuration |
| `scripts`, `.github` | Setup, reproducible checks and CI |
| `docs` | Architecture, deployment and actual validation limits |

See [architecture and data limitations](docs/ARCHITECTURE.md), [contributing](CONTRIBUTING.md) and [security reporting](SECURITY.md) and [dependency review](docs/DEPENDENCIES.md). MIT licensed. Existing unwired type-extraction files are preserved outside the active workspace.

## Prepare your GitHub repository

Source, lockfiles, migrations, documentation and CI belong in Git. `.gitignore` excludes credentials, generated APKs, tool downloads, database files, backups and screenshots. Review `git status --short` and `git diff --cached` before your first commit. Add a remote for a repository you own, push your reviewed commit, and require the Checks workflow on your default branch. Enable dependency alerts, secret scanning and private vulnerability reports. No remote, commit or push is created by these setup files.
# meal_app
