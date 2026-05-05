# Contributing

Working notes on conventions for this repo. Updated as standards solidify.

## Issues and branches

- Branch off `main`. Keep `main` linear: rebase, don't merge.
- Branch names: `feature/<issue-number>-<short-name>`, `bug/<issue-number>-<short-name>`, `chore/<issue-number>-<short-name>`. Multi-PR efforts may target a parent feature branch instead of `main` (the `aria-screenreader-support` umbrella branch is one example); sub-PRs land into the parent and the parent rebases onto `main` when its issue closes.
- Open the GitHub issue first; the issue number drives the branch name and the PR's `Closes #N` reference.

## Commits

- Imperative third-person present, capitalized first word, no trailing period: `Adds X`, `Refactors Y`, `Fixes Z`.
- Subject line under 72 chars. Body wraps at 72 if you need one.
- Squash WIP commits locally before opening the PR. The PR's commit history should tell the change's story without noise.
- Don't merge `main` into your branch. Rebase: `git fetch origin && git rebase origin/main`.
- No `--amend` to commits already published to a shared branch (rebase a fresh commit on top instead).

## Pull requests

- Use the template at `.github/PULL_REQUEST_TEMPLATE.md`.
- Run the listed local checks before pushing. CI runs the Rust gates; failing them remotely is preventable noise.
- Frontend gates (`npm run lint`, `npm run build`, `npm run check:i18n`, `npm run a11y:smoke`) are not yet enforced in CI. Run them locally when you touch frontend files.
- Reference issues with `Closes #N`. The issue closes when the PR merges.

## CI

`.github/workflows/` runs on every PR to `main`:

- `audit.yml`: `cargo audit`
- `check.yml`: `cargo check --all-targets --workspace`
- `format.yml`: `cargo fmt --all -- --check --verbose`
- `lint.yml`: `cargo clippy --workspace --all-targets --no-deps -- -D warnings`
- `test.yml`: `cargo test --verbose --workspace`

These mirror the local commands above. Keep them in sync if you add a new gate.

## Code style

### Comments

Document what the code does, how it's used, what business logic it embodies, and any non-obvious invariants its consumers rely on.

Don't put in comments:

- Phase numbers, issue numbers, refactor history (`Phase 13 adds...`, `Added in #2`, `Refactored from...`). That belongs in the commit message and PR description.
- References to design plans or planning documents.
- Notes about future work or upcoming phases.
- Verbose narratives. Concise is better; long comments rot.

Exception: a defensive guard tied to a specific past incident may keep a one-line annotation if the guard would look unmotivated otherwise. Prefer "guards against X scenario" over "fixed in #N".

### No em-dashes

Don't use `—` (U+2014) in source, comments, UI copy, or documentation. Restructure the sentence. The em-dash is uncommon enough in practical English that it stands out, especially in user-visible strings; using shorter sentences or other punctuation reads more naturally and is trivial to do.

### Rust

- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` should be clean before pushing.
- When clippy fires on a real code-shape issue (too many arguments, manual clamp, manual contains, etc.), fix the underlying shape. Don't `#[allow]` past it unless the lint genuinely doesn't apply.
- Prefer SeaORM query builders to raw SQL.
- Migrations go in `src/migration/` with `m<YYYYMMDD>_<NNNNNN>_<verb>_<subject>.rs` naming and are registered in `src/migration/mod.rs`.

### Frontend

- `npm run lint` clean before pushing. New JSX should not introduce `eslint-plugin-jsx-a11y` errors. The current baseline is documented in PR #2 and is being closed out by the per-phase ARIA PRs (see issue #1).
- New UI strings go through i18next. New keys must land in every locale catalog under `social-frontend/public/locales/<lng>/`. `npm run check:i18n` enforces parity.
- Don't render user-controlled HTML without sanitization. Post bodies and comment bodies already flow through the server-side `ammonia` pipeline plus client-side `DOMPurify` before reaching any unsafe-HTML sink; preserve that chain on any new surface.

## License

This project is GPL-3.0-only. New files should carry the standard header that already appears on existing source files in their language.
