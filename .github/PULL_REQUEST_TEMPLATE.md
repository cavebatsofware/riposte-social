<!-- See CONTRIBUTING.md for branching, commits, comments, and code style. -->

## Summary

<!-- One to three sentences. What does this change do, and why now? -->

Closes #

## Type

<!-- Pick one. Add others as a comment if the PR straddles. -->

- [ ] Feature
- [ ] Bug fix
- [ ] Refactor
- [ ] Tooling / infrastructure
- [ ] Documentation
- [ ] Accessibility
- [ ] Internationalization

## Local checks

CI runs the Rust gates listed below; running them locally first keeps the loop tight and catches noise before the runner does. Frontend gates are not yet in CI; run them yourself when frontend files change.

Backend (always when Rust files change):

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check --all-targets --workspace`
- [ ] `cargo clippy --workspace --all-targets --no-deps -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo audit` (when Cargo.toml or Cargo.lock changed)

Frontend (when `social-frontend/` or `admin-frontend/` files change):

- [ ] `npm run lint`
- [ ] `npm run build` (or `npm run build:social` / `:admin`)
- [ ] `npm run check:i18n` (when any `locales/*.json` changed)
- [ ] `npm run a11y:smoke` against `npm run dev:social` (when `social-frontend/` UI changed)

## Testing

<!-- What was verified, and how. Unit, integration, manual smoke. Mention what was NOT tested. -->

## Screenshots

<!-- For UI changes only. Drag and drop or paste. -->

## Notes

<!-- Migrations, breaking changes, deferred follow-ups, anything a reviewer should know up front. -->
