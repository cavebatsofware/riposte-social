# riposte-social

[![Cargo Check](https://github.com/cavebatsofware/riposte-social/actions/workflows/check.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/check.yml)
[![Cargo Format](https://github.com/cavebatsofware/riposte-social/actions/workflows/format.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/format.yml)
[![Lint](https://github.com/cavebatsofware/riposte-social/actions/workflows/lint.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/lint.yml)
[![Cargo Audit](https://github.com/cavebatsofware/riposte-social/actions/workflows/audit.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/audit.yml)
[![Cargo Test](https://github.com/cavebatsofware/riposte-social/actions/workflows/test.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/test.yml)
[![Cypress](https://github.com/cavebatsofware/riposte-social/actions/workflows/cypress.yml/badge.svg)](https://github.com/cavebatsofware/riposte-social/actions/workflows/cypress.yml)

*riposte* (n.): a quick, sharp reply; in fencing, the counterattack that follows a successful parry.

A self-hosted, user-first social platform. Runs on a home server or scales to EC2/OCI; ships with production-ready containers either way. Accessible (WCAG 2.1 AA), internationalized (6 languages), and designed to federate with the wider fediverse via ActivityPub / ActivityStreams 2.0.

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
- **WCAG 2.1 AA accessibility**: full ARIA roles and labels, keyboard navigation, skip links, focus management, and screen-reader coverage across both SPAs. Cypress a11y suite gates every merge.
- **Internationalization**: UI fully translated in 5 languages (English, German, Spanish, French, Chinese); locale auto-detected from browser preference.
- **8 colorways**: 5 standard themes plus 3 accessible variants (deuteranopia, tritanopia, monochrome); user-selectable at runtime with no reload. See [DESIGN.md](DESIGN.md) for color swatches and token reference.

### In progress / planned

- **Visibility tier alignment**: renaming tiers to `public / followers / local / private` to match ActivityPub semantics before federation begins (#41).
- **ActivityPub / ActivityStreams 2.0 federation**: five-phase implementation (#42-#47): actor documents and WebFinger, activity journal and Outbox, Inbox and HTTP Signatures, delivery queue, and remote content ingestion. When complete, any local user is discoverable as a fediverse actor and can follow or be followed by Mastodon/Pleroma/etc. users.

### Platform foundations

- PostgreSQL 18 + SeaORM with automatic migrations; pg_search (ParadeDB) for BM25 search
- Two-tier rate limiting (forgiving on cache hits, aggressive on errors), request screening, and access logging
- Prometheus metrics and AES-256-GCM encryption at rest
- S3-compatible media storage (AWS, OCI, MinIO)
- Admin panel (React SPA) with email verification, MFA/TOTP, and RBAC

## Quick Start

### Prerequisites

- Rust (latest stable)
- Docker and Docker Compose
- Bun (for frontend builds)
- AWS account with SES configured (for admin email verification)

### Setup

```bash
# Clone and enter the directory
git clone https://github.com/cavebatsofware/riposte-social.git
cd riposte-social

# Create environment configuration
cp .env.example .env
# Edit .env with your values (see Configuration section below)

# Run setup (creates .env if missing, installs npm deps, starts db, runs migrations)
make setup

# Start development server with hot reload
make dev
```

The application runs at `http://localhost:3000`. Run `make help` to see all available commands.

### Endpoints

Public routes:
- `/health` - Health check
- `/metrics` - Prometheus metrics (localhost only)
- `/api/contact` - Contact form submission
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
- `/media/{media_id}` - Post media
- `/album-media/{media_id}` - Album media
- `/access/{code}` - Code-gated document page
- `/access/{code}/download` - Download document
- `/document/{code}` - Alias for access page
- `/document/{code}/download` - Alias for download

Auth routes:
- `/api/auth/register` - Create account
- `/api/auth/login` - Login
- `/api/auth/logout` - Logout
- `/api/auth/verify-email` - Email verification (required before login)
- `/api/auth/config` - Frontend auth configuration (OIDC status)
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
- `/api/site/config` - Current user info and feature flags
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

## Configuration

Copy `.env.example` to `.env` and configure. See `.env.example` for all options with descriptions.

### Required

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `SITE_DOMAIN` | Your domain (used for admin email validation) |
| `SITE_URL` | Full site URL (used in emails and links) |
| `TOTP_ENCRYPTION_KEY` | AES-256 key for MFA secrets (generate with `openssl rand -hex 32`) |

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
| `RATE_LIMIT_PER_MINUTE` | 60 | General request rate limit per IP |
| `BLOCK_DURATION_MINUTES` | 15 | Block duration after exceeding general limit |
| `RATE_LIMIT_GRACE_PERIOD_SECONDS` | 1 | Grace period before the bucket starts charging requests |
| `RATE_LIMIT_CACHE_REFUND_RATIO` | 0.5 | Fraction of token cost refunded for HTTP 304 NOT_MODIFIED responses (cache revalidation) |
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

### Site Settings (seed values)

`SITE_NAME`, `CONTACT_EMAIL`, and `AWS_SES_FROM_EMAIL` serve as **initial seed values** for the database settings table. After the first migration, the admin Settings UI is the source of truth. The app uses a fallback chain: DB value > env var > hardcoded default.

### Feature Gates (managed via admin UI)

The following features can be toggled at runtime through the admin Settings page without restarting the server:

| Setting | Default | Description |
|---------|---------|-------------|
| `admin_registration_enabled` | true | Allow new admin account registration |
| `access_codes_enabled` | true | Enable public code-gated document access |
| `contact_form_enabled` | true | Enable the public contact form endpoint |
| `subscriptions_enabled` | true | Enable the public newsletter subscription endpoint |

When a feature is disabled, public endpoints return 404 and the admin UI hides related navigation.

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
| `PORT` | 3000 | Server listen port |
| `DEV_MODE` | false | Use socket address for IP extraction (for dev without proxy) |
| `RUST_LOG` | - | Tracing/logging level |

## Development

The Makefile provides all development commands. Run `make help` for the full list.

### Common Commands

```bash
make setup            # First-time setup (env, deps, db, migrations)
make dev              # Start with hot reload (Rust + admin frontend)
make dev-no-watch     # Start without hot reload
make clippy           # Run linter
make test             # Run tests (starts test database automatically)
make build            # Build Docker image
```

### Database

```bash
make db-up            # Start PostgreSQL
make db-down          # Stop PostgreSQL
make db-logs          # View database logs
make db-shell         # Open psql shell
make db-migrate       # Run migrations
make db-reset         # Reset database (WARNING: deletes data)
make db-backup        # Backup to ./backups/
make db-restore       # Restore from backup
```

### Testing

Tests use a separate database on port 5433 to avoid conflicts with the development database. The test infrastructure includes mock services for AWS SES, S3, and OIDC.

```bash
make test             # Run all tests (starts test DB automatically)
make test-db-up       # Start test database only
make test-db-down     # Stop test database
make test-db-reset    # Reset test database
make cypress-feature  # Run Cypress feature tests (starts app stack)
make cypress-a11y     # Run Cypress accessibility tests
make cypress-all      # Run all Cypress tests
```

### Frontend Build

Both SPAs are built with Bun.

```bash
make admin-build      # Build admin React SPA
make social-build     # Build social React SPA
make frontend-build   # Build both frontends
```

## Deployment

### Docker Build

```bash
make build            # Build Docker image
make run              # Run container locally (requires ACCESS_CODES env var)
make clean            # Remove local Docker images
```

### ECR Deployment

Configure ECR settings in `.env`:
```bash
ECR_REGISTRY_URL=<account-id>.dkr.ecr.<region>.amazonaws.com
ECR_REPO_NAME=your-repo-name
ECR_REGION=us-east-2
```

Then deploy:
```bash
make check-prereqs    # Verify Docker and AWS CLI setup
make deploy           # Build, tag, and push to ECR
```

### OCIR Deployment

Configure OCIR settings in `.env`:
```bash
OCIR_REGISTRY_URL=<region>.ocir.io/<tenancy-namespace>
OCIR_REPO_NAME=your-repo-name
OCIR_REGION_NAME=us-ashburn-1
OCIR_USERNAME=<tenancy-namespace>/<username>
OCIR_AUTH_TOKEN=<auth-token>
```

Then deploy:
```bash
make deploy-ocir      # Build, tag, and push to OCIR
```

### Production Database

Update `DATABASE_URL` in your production environment:
```bash
DATABASE_URL=postgresql://user:password@your-db-host:5432/dbname
```

Run migrations on first deploy:
```bash
MIGRATE_DB=true cargo run -- migrate
```

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
