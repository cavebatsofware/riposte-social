# Riposte Social

Self-hosted social platform. Rust+Axum+SeaORM, React admin + social SPAs, paradedb (pg18). See [CONTRIBUTING.md](CONTRIBUTING.md) for branching, commits, style, CI. PR template: `.github/PULL_REQUEST_TEMPLATE.md` (run the local checks; CI mirrors them).

## Issue body voice

Developer/planner voice. No "the operator said", "what the user asked for", "called out by". No exhaustive out-of-scope lists, no parallel-alternative dumps when one path is chosen. Tone reference: issues #1, #23-#28. User-stories format only for user-facing changes; skip for pure refactor/infra.

## Pre-merge plugins

- `/code-review:code-review` on every PR. Address Copilot review comments unless false positive.
- `/security-review` for auth, sessions, password/TOTP/OIDC, S3 keys, CSRF, secrets, input validation.
- `/simplify` (or `code-simplifier`) for large PRs or PRs touching established abstractions.

## Owned crates (modify and bump rather than work around)

- `basic-axum-rate-limit` - https://github.com/cavebatsofware/rate-limiter
- `axum-tower-sessions-csrf` - https://github.com/cavebatsofware/axum-tower-sessions-csrf
- `axum-login` (fork) - https://github.com/cavebatsofware/axum-login
- `tower-sessions-sqlx-store` (fork) - https://github.com/cavebatsofware/tower-sessions-stores

## GitHub project

Board: https://github.com/users/cavebatsofware/projects/2 (`gh auth refresh -s project` for `gh project item-add`). Labels: `feature`, `accessibility`, `performance`, `infra`, `ux`.
