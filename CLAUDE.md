# Riposte Social

Self-hosted social platform. Rust+Axum+SeaORM, React admin + social SPAs, paradedb (pg18).
See CONTRIBUTING.md for branching, commits, and CI. PR template: .github/PULL_REQUEST_TEMPLATE.md.

## Stack and architecture

- `src/` Rust/Axum backend; modules: `posts`, `albums`, `auth`, `admin`, `follows`, `engagement`, `invites`, `migration`, `entities`
- `social-frontend/` React SPA (social feed, profiles); built to `social-assets/`. Tokens, theme engine, pickers, and shared components come from the shared `@cavebatsofware/riposte-design-system` + `riposte-pickers` packages, NOT `social-frontend/src`.
- `admin-frontend/` React SPA (admin panel); built to `admin-assets/`
- `tests/` integration tests (axum-test + wiremock); test DB on port 5433

## Development

```bash
make dev            # db-up + frontend build + cargo-watch
make test           # cargo test against test DB
cargo clippy        # lint; CI runs -D warnings
bun run lint        # ESLint social-frontend
bun run check:i18n  # verify i18n keys are in sync
```

- Dev DB: `docker exec riposte-social-db psql -U riposte_social_user -d riposte_social`
- Test DB: `docker exec riposte-social-test-db psql -U riposte_social_test_user -d riposte_social_test`
- Do not mix the two; they are separate containers on separate ports
- `make dev` runs with `--features e2e_testing`; bare `cargo run` breaks socket-address extraction and non-Secure cookies
- Edits to `social-frontend/public/locales/*` need `bun run build:social`; the backend serves from `social-assets/locales/`, not source
- Use bun for all frontend tooling and scripting (deps, package scripts, ad-hoc scripts); never npm, never python
- Design-system deps are `github:` npm deps + a cargo git dep (committed-`dist` model). A clean build needs `riposte-design-system` and `riposte-pickers` pushed; iterate locally with a `file:` / `bun link` / cargo `[patch]` override
- `scripts/build-social.ts` has a resolver plugin forcing a single `react` / `react-i18next` / design-system copy. Do not remove it; the linked packages carry nested copies that otherwise break hooks and the theme context
- Email catalogs are app-owned in `src/email/catalogs/`; the design-system crate supplies the email layout shell, CSS, and catalog merge

## Code conventions

- **AppError**: `NotFound` for hidden or missing domain entities; `AuthError` for credential flows only (login, TOTP, OIDC).
- **SQL-side filters**: push auth/scope filters into the `WHERE` clause; don't fetch broader and filter in Rust.
- **No imagined problems**: don't add `is_empty()` guards, NULL checks on NOT-NULL columns, or "just in case" branches the framework/schema already handles.
- **No legacy workarounds**: pre-MVP, no production data. No compat shims, NULL fallbacks, or "claim ownership of legacy rows" branches.
- **Comments**: not commit messages, describe current behavior, never narrate the change. Explain once at the implementation, never at call sites or wrappers (do not propagate the explanation up the call chain). Default to none; otherwise the minimum that clarifies what non-obvious code does, with a fuller WHY only when the situation requires it. Verbosity is a cost, not a virtue.
- **No em-dash** (U+2014) anywhere: comments, prose, PR bodies, commit messages, any output. Use commas, parentheses, colons, semicolons, or rephrase.

## Issues and PRs

**Voice**: developer/planner tone. No "the operator said", "called out by". No exhaustive out-of-scope lists. User-stories format only for user-facing changes; skip for refactor/infra. Tone reference: issues #1, #23-#28.

**Pre-merge**: run `/code-review:code-review` on every PR (address Copilot comments unless false positive); `/security-review` for auth/sessions/TOTP/OIDC/S3/CSRF/secrets/input validation; `/simplify` for large PRs or PRs touching established abstractions.

**Before publishing**: show the exact issue or PR text in conversation and wait for explicit confirmation before `gh issue create` or `gh pr create`. Plan approval is not enough.

**Git**: never push to remote; commit and stop (user pushes after local testing). No Co-Authored-By trailers unless explicitly asked. Branch upstream must be `origin/<same-name>`, never the parent: `git switch --no-track -c feature/X origin/parent`, then `git push -u origin feature/X`.

## References

**Owned packages** (modify and bump rather than work around):
- `basic-axum-rate-limit` https://github.com/cavebatsofware/rate-limiter
- `axum-tower-sessions-csrf` https://github.com/cavebatsofware/axum-tower-sessions-csrf
- `@cavebatsofware/riposte-design-system` (polyglot: npm package + Rust crate) https://github.com/cavebatsofware/riposte-design-system
- `@cavebatsofware/riposte-pickers` (npm; depends on riposte-design-system) https://github.com/cavebatsofware/riposte-pickers

**Babysitting until upstream updates land** (use as-is; don't modify):
- `axum-login` (fork) https://github.com/cavebatsofware/axum-login
- `tower-sessions-sqlx-store` (fork) https://github.com/cavebatsofware/tower-sessions-stores
- `sea-query` (fork) https://github.com/cavebatsofware/sea-query: wired via
  `[patch.crates-io]` in Cargo.toml, branch `fix-bracket-tokenizer-0.32.7` (the
  0.32.x line sea-orm 1.1 needs). Carries SeaQL/sea-query PR #1074 (open
  upstream): fixes the Postgres tokenizer treating `[` as a quote start, which
  corrupts `$N` placeholders in statements mixing array subscripts like
  `(ARRAY_AGG(...))[1]` with bound params (src/albums/queries.rs). Without it the
  album stats query is wrong. Drop the patch once #1074 ships in a released
  sea-query that sea-orm depends on.

**GitHub project**: https://github.com/users/cavebatsofware/projects/2
(`gh auth refresh -s project` for `gh project item-add`). Labels: `feature`, `accessibility`, `performance`, `infra`, `ux`.
