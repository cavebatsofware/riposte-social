# riposte-social

[![Cargo Check](https://github.com/cavebatsofware/riposte-social/actions/workflows/check.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/check.yml)
[![Cargo Format](https://github.com/cavebatsofware/riposte-social/actions/workflows/format.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/format.yml)
[![Lint](https://github.com/cavebatsofware/riposte-social/actions/workflows/lint.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/lint.yml)
[![Cargo Audit](https://github.com/cavebatsofware/riposte-social/actions/workflows/audit.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/audit.yml)
[![Cargo Test](https://github.com/cavebatsofware/riposte-social/actions/workflows/test.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/test.yml)
[![Cypress](https://github.com/cavebatsofware/riposte-social/actions/workflows/cypress.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/cypress.yml)

*riposte* (n.): a quick, sharp reply; in fencing, the counterattack that follows a successful parry.

A self-hosted, user-first social platform. Runs on a home server or scales to EC2/OCI; ships with production-ready containers either way. Accessible (WCAG 2.1 AA), internationalized (5 languages), and designed to federate with the wider fediverse via ActivityPub / ActivityStreams 2.0. An optional business module adds a storefront and order intake, so the same host can serve a community feed and a small commerce site side by side.

> **Status:** Core social features complete; ActivityPub federation in progress. Not yet production-ready.

### Implemented

- **Three-tier access**: administrator, poster (trusted authors), and commenter (invite-only). Anonymous visitors see a public feed.
- **Invite flow**: admins issue invite codes; invitees land on a welcome splash; a persistent cookie re-surfaces it until accepted or expired.
- **Markdown posts with media**: compose with links and inline images; attach photos and video (S3-backed).
- **Reactions and comments**: any authenticated user can react or comment; admins moderate via soft-delete.
- **Follower / following social graph**: local follow relationships; visibility tiers keyed to follow state.
- **BM25 full-text search**: powered by pg_search / ParadeDB on top of PostgreSQL 18.
- **OIDC/Keycloak SSO**: primary auth mode, federates to any OIDC provider. Local password + TOTP fallback when OIDC is disabled.
- **Facebook export import**: drag-drop the FB data export ZIP; the server dedupes, re-hosts media, and preserves publish dates.
- **Optional business module**: a storefront + order-intake surface gated behind the `business` cargo feature and the `APP_ROLE` setting, with Cloudflare Turnstile captcha, optional Twilio phone verification, and a choice of SES, SendGrid, or Resend for email. See [Business module](#business--storefront-module).
- **WCAG 2.1 AA accessibility**: full ARIA roles and labels, keyboard navigation, skip links, focus management, and screen-reader coverage across both SPAs. Cypress a11y suite gates every merge.
- **Internationalization**: UI fully translated in 5 languages (English, German, Spanish, French, Chinese); locale auto-detected from browser preference.
- **8 colorways**: 5 standard themes plus 3 accessible variants (deuteranopia, tritanopia, monochrome); user-selectable at runtime with no reload, with the per-site default (colorway + light/dark shade) configurable from the admin Settings UI. See [DESIGN.md](DESIGN.md) for color swatches and token reference.

### In progress / planned

- **Visibility tier alignment**: renaming tiers to `public / followers / local / private` to match ActivityPub semantics before federation begins (#41).
- **ActivityPub / ActivityStreams 2.0 federation**: five-phase implementation (#42-#47): actor documents and WebFinger, activity journal and Outbox, Inbox and HTTP Signatures, delivery queue, and remote content ingestion. When complete, any local user is discoverable as a fediverse actor and can follow or be followed by Mastodon/Pleroma/etc. users.

### Platform foundations

- PostgreSQL 18 + SeaORM with automatic migrations; pg_search (ParadeDB) for BM25 search
- Two-tier rate limiting (forgiving on cache hits, aggressive on errors), request screening, and access logging
- Prometheus metrics and AES-256-GCM encryption at rest
- S3-compatible media storage (AWS, OCI, MinIO)
- Cloudflare Turnstile captcha on the public contact and order forms
- Admin panel (React SPA) with email verification, MFA/TOTP, and RBAC

## Quick Start

### Prerequisites

- Rust (latest stable, via rustup)
- Docker and Docker Compose
- Bun (all frontend builds and the deploy CLI)
- AWS account with SES configured (for admin email verification)
- For builds/deploys with the CLI: `sops` + `age` (secret management). A `flake.nix` dev shell provides bun, sops, age, the Docker client, and rustup: `nix develop`.

### A site manifest

The repo is configured per deployment with a typed **site manifest** under `sites/<name>.ts` (the `SiteManifest` type in `sites/manifest.ts`): the non-secret identity for one deployment, i.e. its domain, app role, image tag, compose service, database, optional storefront, and the names of its secrets. The bun CLI reads a site by name; secret *values* live encrypted (see [Secrets](#secrets-sops--age)), never in the manifest. The repo ships example manifests you can copy.

### Local development

```bash
git clone https://github.com/cavebatsofware/riposte-social.git
cd riposte-social

# Copy and edit local env (or use a manifest's dev env)
cp .env.example .env

# Start the dev DB, build both SPAs, and watch Rust + admin + social with hot reload
bun tooling/cli.ts dev <site>
```

The application runs at `http://localhost:3000`. Run `bun tooling/cli.ts` with no arguments for the full command list.

> The legacy `Makefile` still exists but is **deprecated** and prints a notice; it will be removed. Use the bun CLI.

### Endpoints

Public routes:
- `/health` - Health check
- `/metrics` - Prometheus metrics (localhost only)
- `/api/contact` - Contact form submission (Turnstile-protected when a secret is configured)
- `/api/orders` - Order intake (business module; Turnstile-protected)
- `/api/subscribe` - Newsletter subscription
- `/api/subscribe/verify` - Verify subscription token
- `/api/feed` - Public post feed
- `/api/posts/{id}` - Single post
- `/api/posts/{id}/comments` - Post comments
- `/api/posts/{id}/reactions` - Post reactions
- `/api/albums` - Album list
- `/api/albums/{id}` - Single album
- `/api/categories` - Category list
- `/api/users/{user_id}/followers` - Follower list
- `/api/users/{user_id}/following` - Following list
- `/api/site/config` - Per-tier site configuration (see below)
- `/media/{media_id}` - Post media
- `/album-media/{media_id}` - Album media
- `/access/{code}` - Code-gated document page
- `/access/{code}/download` - Download document
- `/document/{code}` - Alias for access page
- `/document/{code}/download` - Alias for download

`/api/site/config` is public and tiered: every caller gets `site_name`, `public_feed_enabled`, the theme defaults (`default_colorway`, `default_shade`), and, when configured, `shop_url` / `turnstile_site_key` / `commerce_enabled`; posters and admins additionally see the gates relevant to them. The SPAs read it to keep features hidden until confirmed enabled.

Auth routes:
- `/api/auth/register` - Create account
- `/api/auth/login` - Login
- `/api/auth/logout` - Logout
- `/api/auth/verify-email` - Email verification (required before login)
- `/api/auth/config` - Frontend auth configuration (OIDC status, login/account URLs, and `site_domain` for the admin email check)
- `/api/auth/csrf-token` - CSRF token for forms
- `/api/auth/forgot-password` - Request password reset
- `/api/auth/reset-password` - Complete password reset

OIDC routes (when `OIDC_ENABLED=true`):
- `/api/auth/oidc/login` - Redirect to identity provider (shared by all user tiers)
- `/api/auth/oidc/callback` - Handle provider callback; redirects administrators to `/admin`, posters and commenters to `/`

Invite routes:
- `/invite/{code}` - Invite landing page
- `/api/invites/current` - Check pending invite state
- `/api/invites/confirm` - Accept an invite
- `/api/auth/logout/invite` - Clear pending invite and log out

Current-user routes (authenticated):
- `/api/me/password` - Change password
- `/api/me/mfa/setup` - Initiate TOTP setup
- `/api/me/mfa/confirm-setup` - Confirm TOTP enrollment
- `/api/me/mfa/disable` - Disable TOTP
- `/api/me/follows/state` - Follow state for a set of users
- `/api/auth/mfa/verify` - Verify TOTP code at login

Admin panel routes (require administrator role):
- `/api/admin/access-codes` - Manage access codes (CRUD + file upload)
- `/api/admin/access-logs` - View access logs
- `/api/admin/dashboard/metrics` - Dashboard metrics
- `/api/admin/admin-users` - Manage admin users
- `/api/admin/settings` - Site and feature settings
- `/api/admin/invite-codes` - Manage invite codes
- `/api/admin/imports` - Facebook export imports
- `/api/admin/moderation` - Content moderation (soft-delete)
- `/api/categories` (POST/PUT/DELETE) - Category management (admin only)

SPAs:
- `/admin` - Admin panel (React SPA, serves `index.html` for all `/admin/*` paths)
- `/` and all other paths - Social feed SPA (fallback)

The shop role (business module) serves a separate storefront on its own port; see [Business module](#business--storefront-module).

## Configuration

Copy `.env.example` to `.env` and configure. See `.env.example` for all options with descriptions.

### Required

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `SITE_DOMAIN` | Your domain (used for admin email validation; served to the admin SPA at runtime via `/api/auth/config`) |
| `SITE_URL` | Full site URL (used in emails and links) |
| `SECURE_VALUES_KEY` | AES-256 key for encrypted-at-rest values: TOTP secrets, single-use tokens, encrypted `settings` rows (generate with `openssl rand -hex 32`). Resolved through the secret file-path chain (see Secret delivery). |
| `APP_ROLE` | `social`, `shop`, or `both`. Selects which server(s) run; `shop`/`both` require the `business` build. Defaults to social. |

### Secret delivery

`SECURE_VALUES_KEY`, the database password (`POSTGRES_PASSWORD`), and
`OIDC_CLIENT_SECRET` resolve through a file-path chain so they never have
to live in the process environment. For each secret `<NAME>`, the server
checks, in order:

1. `<NAME>_FILE` env var, read as a file path (explicit override).
2. `$CREDENTIALS_DIRECTORY/<name>` (systemd `LoadCredentialEncrypted`).
3. `/run/secrets/<name>` (Docker `secrets:` default).
4. `<NAME>` env var (fallback for local dev and CI).

`<name>` is the lowercased env name (e.g. `secure_values_key`). A single
trailing newline in the file is trimmed, so `openssl rand -hex 32 > keyfile`
works as written. Per-topology mounts:

- **Docker Compose**: define a `secrets:` block and mount it; the file
  appears at `/run/secrets/<name>`.
- **systemd**: `LoadCredentialEncrypted=<name>:/path/to/secret`; the unit
  sees it under `$CREDENTIALS_DIRECTORY/<name>`.
- **Any platform**: point `<NAME>_FILE` at a mounted file.

`DATABASE_URL` is used verbatim when set (keep it for local dev and CI).
When unset, the URL is assembled from `DATABASE_HOST`, `DATABASE_PORT`,
`POSTGRES_USER`, `POSTGRES_DB`, and the resolved `POSTGRES_PASSWORD`; set
`DATABASE_SSLMODE` if the assembled URL needs a TLS mode (a verbatim
`DATABASE_URL` carries its own `sslmode`).

AWS credentials come from the AWS SDK's own provider chain, not app env
vars: a shared-credentials file (`AWS_SHARED_CREDENTIALS_FILE` pointed at a
mounted secret, or `~/.aws/credentials`) or instance / container role
credentials.

### AWS (required for admin accounts)

| Variable | Description |
|----------|-------------|
| `AWS_SES_FROM_EMAIL` | Verified SES sender address (also seeds DB `from_email` setting) |
| `S3_BUCKET_NAME` | S3 bucket for document storage |
| `S3_ENDPOINT_URL` | Custom S3 endpoint (optional, for OCI/MinIO) |
| `S3_REGION` | Override AWS region for S3 only (optional) |
| `S3_FORCE_PATH_STYLE` | Force path-style addressing for OCI/MinIO (default: false) |

### Security & Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_PER_MINUTE` | 120 | General request rate limit per IP |
| `BLOCK_DURATION_MINUTES` | 15 | Block duration after exceeding general limit |
| `RATE_LIMIT_GRACE_PERIOD_SECONDS` | 1 | Grace period before the bucket starts charging requests |
| `RATE_LIMIT_CACHE_REFUND_RATIO` | 0.75 | Fraction of token cost refunded for HTTP 304 NOT_MODIFIED responses (cache revalidation) |
| `RATE_LIMIT_AUTH_REFUND_RATIO` | 0.5 | Fraction refunded on the general bucket for a successful authenticated request |
| `RATE_LIMIT_ERROR_PENALTY` | 2.0 | Extra tokens charged for 4xx/5xx responses on top of the base 1-token cost |
| `AUTH_RATE_LIMIT_PER_MINUTE` | 5 | Stricter limit for auth endpoints |
| `AUTH_BLOCK_DURATION_MINUTES` | 30 | Block duration after exceeding auth limit |
| `AUTH_RATE_LIMIT_GRACE_PERIOD_SECONDS` | 0 | Grace period for the auth bucket (default 0 so brute-force consumes immediately) |
| `AUTH_RATE_LIMIT_CACHE_REFUND_RATIO` | 0.0 | Refund ratio for auth 304 (default 0.0 since auth is stateful) |
| `AUTH_RATE_LIMIT_ERROR_PENALTY` | 4.0 | Extra tokens for failed auth (a 5-req/min bucket blocks after one failed attempt at 4.0) |

The general bucket follows "forgiving on cache hits, aggressive on errors": a re-poll that hits the browser cache (304 NOT_MODIFIED) refunds part of its cost, while 4xx probing depletes the bucket faster, so legitimate browsing scales while abuse gets blocked sooner. The auth bucket applies the same asymmetry to credential testing so failed logins cost much more than successful ones.

### Access Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_ACCESS_LOGGING` | true | Log access attempts to database |
| `LOG_SUCCESSFUL_ATTEMPTS` | true | Include successful attempts (false reduces DB writes) |
| `ACCESS_LOG_RETENTION_DAYS` | 1 | Days to retain logs before automatic cleanup |

### Settings (admin UI is the source of truth)

Most configuration is database-backed and edited at runtime in the admin **Settings** page, with no restart. A handful of env vars (`SITE_NAME`, `CONTACT_EMAIL`, `AWS_SES_FROM_EMAIL`, `SITE_DOMAIN`, theme defaults) serve as **initial seed values**; after first migration the DB is authoritative, via the chain: DB value > env var > hardcoded default.

The Settings page is the complete, current list. Representative keys:

| Setting | Purpose |
|---------|---------|
| `admin_registration_enabled` | Allow new admin account registration |
| `access_codes_enabled` | Enable public code-gated document access |
| `contact_form_enabled` | Enable the public contact form endpoint |
| `subscriptions_enabled` | Enable the public newsletter endpoint |
| `public_feed_enabled` | Allow anonymous reads (off = invite-only) |
| `commenter_invites_enabled` | Master switch for the invite system |
| `poster_posting_enabled`, `poster_category_management_enabled` | Poster-tier capabilities |
| `fb_import_enabled` | Allow new Facebook archive uploads |
| `site_name`, `site_domain`, `contact_email`, `from_email` | Site identity |
| `default_colorway`, `default_shade` | Per-site theme default (see below) |
| `max_image_dimension` | Upload size guard |
| Business keys | See [Business module](#business--storefront-module) |

When a feature is disabled, public endpoints return 404 and the admin UI hides related navigation.

#### Theme defaults

`default_colorway` (e.g. `avernus`, `forest`, `plum`) and `default_shade` (`light`, `dark`, or blank = follow the visitor's OS) set the theme a fresh visitor sees. A visitor's own pick always wins and is never overwritten; with no stored pick the site default applies and is not persisted, so changing the setting later reaches everyone who hasn't chosen. The resolution lives in the shared `@cavebatsofware/riposte-design-system` `ThemeProvider`, so the social, admin, and storefront frontends behave identically.

### OIDC Authentication (optional)

When `OIDC_ENABLED=true`, local password authentication is disabled and users authenticate via the configured identity provider (e.g., Keycloak). Roles are synced from the ID token on each login.

| Variable | Default | Description |
|----------|---------|-------------|
| `OIDC_ENABLED` | false | Enable OIDC SSO |
| `OIDC_ISSUER_URL` | - | Provider issuer URL |
| `OIDC_CLIENT_ID` | - | OAuth2 client ID |
| `OIDC_CLIENT_SECRET` | - | OAuth2 client secret |
| `OIDC_REDIRECT_URI` | - | Callback URL (must match provider config) |
| `OIDC_SCOPES` | `openid profile email` | Scopes to request |
| `OIDC_ROLE_CLAIM` | `realm_access.roles` | JSON path to roles in ID token |
| `OIDC_ADMIN_ROLE` | `admin` | Role name that maps to administrator |

### Other

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | 3000 | Social server listen port |
| `SHOP_PORT` | 3001 | Storefront listen port when `APP_ROLE=both` (a shop-only role uses `PORT`) |
| `DEV_MODE` | false | Use socket address for IP extraction (for dev without proxy) |
| `RUST_LOG` | - | Tracing/logging level |

## Business / storefront module

The business module is compiled behind the `business` cargo feature and run by setting `APP_ROLE`:

- `social` (default): only the community network.
- `shop`: only the storefront + order intake.
- `both`: run both on one host, social on `PORT` and the storefront on `SHOP_PORT`.

The storefront frontend is your own static site (any build that emits static HTML/JS), staged into `shop-assets/` and served by the shop server; its public origin is the `shop_url` setting. Order submissions hit `/api/orders` and are guarded by Cloudflare Turnstile. Optional add-ons: Twilio phone verification for orders, order SMS notifications, and a choice of SES, SendGrid, or Resend for transactional email.

Business-module settings (admin Settings UI):

| Setting | Purpose |
|---------|---------|
| `business_enabled` | Master switch for the commerce surface |
| `shop_url` | Public storefront origin (also exposed via `/api/site/config`) |
| `turnstile_site_key` / `secret_turnstile` | Cloudflare Turnstile public key + secret (captcha on contact + orders) |
| `order_statuses` | Configurable order status list |
| `phone_verification_enabled`, `twilio_account_sid`, `secret_twilio_auth_token` | Twilio phone verification |
| `order_sms_enabled`, `secret_order_sms_to` | Order SMS notifications |
| `email_provider`, `secret_sendgrid_api_key`, `secret_resend_api_key` | Email provider selection (SES, SendGrid, or Resend) |

## Development

The bun CLI (`tooling/cli.ts`) is the single interface for development and deployment. Run it with no arguments for the full list.

```bash
bun tooling/cli.ts dev <site>     # dev DB up + build SPAs + watch Rust/admin/social
bun tooling/cli.ts test <site>    # test DB up + cargo test
bun tooling/cli.ts db <site> up   # up | down | logs | shell | migrate | reset
bun tooling/cli.ts show <site>    # print the resolved, validated manifest
bun tooling/cli.ts sites          # list known sites
```

Linting/typechecks run directly: `cargo clippy` (CI runs `-D warnings`), `bun run lint` (ESLint), `bun run check:i18n` (i18n key sync).

### Testing

Tests use a separate database on port 5433 to avoid conflicts with the development database. The test infrastructure includes mock services for AWS SES, S3, and OIDC.

```bash
bun tooling/cli.ts test <site>    # cargo tests against the test DB
bun run e2e:feature               # Cypress feature suite
bun run a11y:smoke                # Cypress accessibility smoke suite
```

### Frontends

Both SPAs are built with Bun and consume the shared `@cavebatsofware/riposte-design-system` + `riposte-pickers` packages (tokens, theme engine, pickers). The CLI builds them as part of `dev`/`build`; to build standalone: `bun run build:admin`, `bun run build:social`.

## Deployment

The production stack is a single host fronted by a reverse proxy or tunnel: `docker-compose.prod.yml` runs PostgreSQL plus one app container per deployment. Non-secret runtime config comes from a per-deployment env file; secrets are file-based Docker secrets resolved via the file-path chain above.

The CLI drives build, deploy, and provisioning per site:

```bash
bun tooling/cli.ts build <site>      # build the image + verify the baked storefront identity
bun tooling/cli.ts deploy <site>     # build (with verify) + atomic tag/push to GHCR (same image id)
bun tooling/cli.ts provision <site>  # decrypt secrets, ensure the DB role/database, bring the service up
```

`deploy` pushes the exact built image (no stale-tag re-tag). `provision` is idempotent: it materializes the runtime secrets, creates the role/database if absent (re-syncing the role password), and starts the compose service, which runs SeaORM migrations on boot via `MIGRATE_DB=true`. The migration runner is also available standalone: `MIGRATE_DB=true cargo run -- migrate`.

### Secrets (SOPS + age)

Per-deployment secrets are encrypted at rest with [SOPS](https://github.com/getsops/sops) + [age](https://github.com/FiloSottile/age). A `.sops.yaml` names the age recipient; the encrypted file is local-only (gitignored). The CLI manages them:

```bash
bun tooling/cli.ts secrets <site> gen      # new site: generate values
bun tooling/cli.ts secrets <site> import   # existing deploy: encrypt current plaintext (preserves keys)
bun tooling/cli.ts secrets <site> edit     # open decrypted in $EDITOR
bun tooling/cli.ts secrets <site> decrypt  # materialize runtime secrets for compose
```

Point `SOPS_AGE_KEY_FILE` at your age key (the `flake.nix` dev shell defaults it). At deploy time `provision`/`deploy` decrypt as needed; plaintext only ever lands in the gitignored secrets mount.

> The `Makefile` retains the older ECR (`make deploy`) and OCIR (`make deploy-ocir`) targets, but it is **deprecated** and will be removed. Use the CLI.

## Security

Riposte is designed to be safely run by people who aren't full-time sysadmins. Most of the protections that typical self-hosted web applications leave to external tools are built in at the application layer.

### Rate limiting and abuse protection

Most self-hosted applications rely entirely on external tools like fail2ban to block login attacks and scanner traffic. Riposte builds this in directly. A two-tier rate limiter runs on every request before it reaches application code:

- The **general bucket** is forgiving toward normal browsing (cached responses are cheaper) but aggressive toward probing: 4xx errors drain the budget faster, so a scanner exhausts its allowance quickly without affecting legitimate users.
- The **auth bucket** is stricter and applies a heavy penalty on failed login attempts. A brute-force attack burns through its budget after a handful of failures rather than running indefinitely.

**Request screening** runs before rate limiting: common scanner patterns (PHP/WordPress probes, JNDI injection strings, known bad user agents) are rejected immediately, before they consume any tokens.

Running fail2ban alongside Riposte is still a good idea. But a freshly deployed instance is not an open target while you get the rest of your stack configured, and less experienced self-hosters get meaningful protection out of the box with no additional tooling.

### Login security

Riposte supports three login modes, usable in combination:

- **Password login**: passwords are hashed with Argon2id, the current recommended standard for secure password storage.
- **Two-factor authentication (MFA/TOTP)**: an optional second step requiring a time-limited code from any authenticator app (Google Authenticator, Authy, 1Password, etc.) in addition to the password. Even if a password is stolen, an attacker cannot log in without the code. Secrets are encrypted at rest; three wrong codes in a row locks the account.
- **Single sign-on (OIDC)**: for households or small organizations already using an identity provider (Keycloak, Authentik, Google Workspace, etc.), OIDC delegates authentication entirely to that system. Users log in with their existing account; Riposte never sees their password. When OIDC is enabled, local password login is disabled.

### Session and CSRF protection

Sessions are stored in PostgreSQL, not in client-side cookies. They expire after a day of inactivity and are invalidated immediately on password change, email change, or MFA toggle; a stolen session cookie does not persist after a credential rotation. All state-changing endpoints require a CSRF token, blocking cross-site request forgery.

### Encryption at rest

TOTP secrets, email verification tokens, and password reset tokens are encrypted with AES-256-GCM using a key you supply at startup. The application refuses to start if the key is absent or malformed.

### Audit logging and metrics

Every significant access attempt is logged with IP address, user agent, action type, and outcome. Retention is configurable; the database cleans up old entries automatically. Prometheus metrics are exposed at `/metrics` but only to localhost; the endpoint actively rejects proxied requests. Restrict it further at your reverse proxy or firewall.

### Role-based access control

Three user tiers (administrator, poster, commenter) with hard permission boundaries. Admin CRUD endpoints reject non-administrator sessions at the API layer regardless of frontend state.

## License

This project is licensed under [GPL-3.0-only](https://www.gnu.org/licenses/gpl-3.0.html).
