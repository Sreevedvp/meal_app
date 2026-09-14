# Contributing

Use Rust 1.96.0 and Dioxus CLI 0.7.10. Start with the README's local setup. Keep money in integer paise and enforce plan constraints in `meal-domain`; the API must independently validate client input.

Run `./scripts/check.sh` and Clippy before submitting a change. Set `DATABASE_URL` to a disposable PostgreSQL database to include integration tests. For UI changes, build the web client, start the server and run `npm ci && npm run test:browser`. The browser scripts create fixture plans and screenshots in ignored `outputs/`.

Keep migrations additive where possible. Never edit an applied migration. Catalogue changes require a new version and source/provenance review; existing saved calendars use historical catalogue snapshots. Authenticated queries must constrain `owner_id`; updates and deletion must retain revision checks.

Describe the concrete problem, resulting behavior, validation and remaining limits in pull requests. Do not include generated builds, secrets or real user records.
