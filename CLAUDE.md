# Riposte Social

Self-hosted social platform. Rust+Axum+SeaORM, React admin + social SPAs, paradedb (pg18).
See CONTRIBUTING.md for branching, commits, and CI. PR template: .github/PULL_REQUEST_TEMPLATE.md.

## Stack and architecture

- `src/` — Rust/Axum backend; modules: `posts`, `albums`, `auth`, `admin`, `follows`, `engagement`, `invites`, `migration`, `entities`
- `social-frontend/` — React SPA (social feed, profiles); built to `social-assets/`
- `admin-frontend/` — React SPA (admin panel); built to `admin-assets/`
- `tests/` — integration tests (axum-test + wiremock); test DB on port 5433

## Development

```bash
make dev            # db-up + frontend build + cargo-watch
make test           # cargo test against test DB
cargo clippy        # lint; CI runs -D warnings
npm run lint        # ESLint social-frontend
npm run check:i18n  # verify i18n keys are in sync
```

- Dev DB: `docker exec riposte-social-db psql -U riposte_social_user -d riposte_social`
- Test DB: `docker exec riposte-social-test-db psql -U riposte_social_test_user -d riposte_social_test`
- Do not mix the two; they are separate containers on separate ports
- `make dev` runs with `--features e2e_testing`; bare `cargo run` breaks socket-address extraction and non-Secure cookies
- Edits to `social-frontend/public/locales/*` need `npm run build:social`; the backend serves from `social-assets/locales/`, not source

## Code conventions

- **AppError**: `NotFound` for hidden or missing domain entities; `AuthError` for credential flows only (login, TOTP, OIDC).
- **SQL-side filters**: push auth/scope filters into the `WHERE` clause; don't fetch broader and filter in Rust.
- **No imagined problems**: don't add `is_empty()` guards, NULL checks on NOT-NULL columns, or "just in case" branches the framework/schema already handles.
- **No legacy workarounds**: pre-MVP, no production data. No compat shims, NULL fallbacks, or "claim ownership of legacy rows" branches.
- **Comments**: document behavior once at the implementation; never at call sites or wrappers. Default to no comment; add one only when the WHY is non-obvious.
- **No em-dash** (U+2014) anywhere: comments, prose, PR bodies, commit messages, any output. Use commas, parentheses, colons, semicolons, or rephrase.

## Issues and PRs

**Voice**: developer/planner tone. No "the operator said", "called out by". No exhaustive out-of-scope lists. User-stories format only for user-facing changes; skip for refactor/infra. Tone reference: issues #1, #23-#28.

**Pre-merge**: run `/code-review:code-review` on every PR (address Copilot comments unless false positive); `/security-review` for auth/sessions/TOTP/OIDC/S3/CSRF/secrets/input validation; `/simplify` for large PRs or PRs touching established abstractions.

**Before publishing**: show the exact issue or PR text in conversation and wait for explicit confirmation before `gh issue create` or `gh pr create`. Plan approval is not enough.

**Git**: never push to remote; commit and stop (user pushes after local testing). No Co-Authored-By trailers unless explicitly asked. Branch upstream must be `origin/<same-name>`, never the parent: `git switch --no-track -c feature/X origin/parent`, then `git push -u origin feature/X`.

## References

**Owned crates** (modify and bump rather than work around):
- `basic-axum-rate-limit` — https://github.com/cavebatsofware/rate-limiter
- `axum-tower-sessions-csrf` — https://github.com/cavebatsofware/axum-tower-sessions-csrf

**Babysitting until upstream updates land** (use as-is; don't modify):
- `axum-login` (fork) — https://github.com/cavebatsofware/axum-login
- `tower-sessions-sqlx-store` (fork) — https://github.com/cavebatsofware/tower-sessions-stores

**GitHub project**: https://github.com/users/cavebatsofware/projects/2
(`gh auth refresh -s project` for `gh project item-add`). Labels: `feature`, `accessibility`, `performance`, `infra`, `ux`.
