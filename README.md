# riposte-social

[![Cargo Check](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/check.yml/badge.svg)](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/check.yml)
[![Cargo Format](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/format.yml/badge.svg)](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/format.yml)
[![Lint](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/lint.yml/badge.svg)](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/lint.yml)
[![Cargo Audit](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/audit.yml/badge.svg)](https://github.com/social.cavebatsofware.com/riposte-social/actions/workflows/audit.yml)

> Generated from [cavebatsofware-site-template](https://github.com/cavebatsofware/cavebatsofware-site-template) via `cargo generate`.

A self-hosted social media sharing platform.

Features:
- Code-gated document access for controlled distribution (e.g., resumes, proposals)
- Admin panel (React SPA) with email verification, MFA/TOTP, and RBAC
- OIDC/Keycloak SSO integration (optional, replaces local auth when enabled)
- Runtime feature gates for access codes, contact form, and subscriptions
- PostgreSQL database with SeaORM and automatic migrations
- Two-tier rate limiting, request screening, and access logging
- Prometheus metrics and AES-256-GCM encryption at rest
- S3-compatible document storage (AWS, OCI, MinIO)

## Quick Start

### Prerequisites

- Rust (latest stable)
- Docker and Docker Compose
- Node.js and npm (for admin panel build)
- AWS account with SES configured (for admin email verification)

### Setup

```bash
# Clone and enter the directory
git clone https://github.com/social.cavebatsofware.com/riposte-social.git
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
- `/access/{code}` - Code-gated document page
- `/access/{code}/download` - Download document
- `/document/{code}` - Alias for access page
- `/document/{code}/download` - Alias for download
- `/health` - Health check
- `/metrics` - Prometheus metrics (localhost only)
- `/api/contact` - Contact form submission
- `/api/subscribe` - Newsletter subscription
- `/api/subscribe/verify` - Verify subscription token

Admin auth routes:
- `/api/admin/register` - Create admin account
- `/api/admin/login` - Login
- `/api/admin/logout` - Logout
- `/api/admin/verify-email` - Email verification (required before login)
- `/api/admin/auth-config` - Frontend auth configuration (OIDC status)
- `/api/admin/me` - Current user info and feature flags
- `/api/admin/csrf-token` - CSRF token for forms
- `/api/admin/forgot-password` - Request password reset
- `/api/admin/forgot-password/verify-mfa` - MFA during password reset
- `/api/admin/reset-password` - Complete password reset
- `/api/admin/change-password` - Change current password
- `/api/admin/mfa/setup` - Initiate TOTP setup
- `/api/admin/mfa/confirm-setup` - Confirm TOTP enrollment
- `/api/admin/mfa/verify` - Verify TOTP code at login
- `/api/admin/mfa/disable` - Disable TOTP

OIDC routes (when `OIDC_ENABLED=true`):
- `/api/admin/oidc/login` - Redirect to identity provider
- `/api/admin/oidc/callback` - Handle provider callback

Admin panel routes (require administrator role):
- `/api/admin/access-codes` - Manage access codes (CRUD + file upload)
- `/api/admin/access-logs` - View access logs and dashboard metrics
- `/api/admin/admin-users` - Manage admin users
- `/api/admin/settings` - Site and feature settings

Admin SPA:
- `/admin` - Admin panel (React SPA, serves `index.html` for all `/admin/*` paths)

### Upstream Template Updates

This project was scaffolded from the [cavebatsofware-site-template](https://github.com/cavebatsofware/cavebatsofware-site-template) repository. To pull in improvements from upstream, add it as a remote and cherry-pick commits:

```bash
git remote add template https://github.com/cavebatsofware/cavebatsofware-site-template.git
git fetch template
# inspect commits, then cherry-pick selectively:
git cherry-pick <commit-sha>
```

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
| `RATE_LIMIT_PER_MINUTE` | 30 | General request rate limit per IP |
| `BLOCK_DURATION_MINUTES` | 15 | Block duration after exceeding general limit |
| `AUTH_RATE_LIMIT_PER_MINUTE` | 5 | Stricter limit for auth endpoints |
| `AUTH_BLOCK_DURATION_MINUTES` | 30 | Block duration after exceeding auth limit |

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
```

### Frontend Build

```bash
make admin-build      # Build admin React SPA
make frontend-build   # Build all frontends
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

## Security Features

- **Authentication** - Local password auth (Argon2 hashing) or OIDC/Keycloak SSO
- **MFA/TOTP** - Time-based one-time passwords with QR code enrollment; AES-256-GCM encrypted secrets at rest; lockout after 3 failed attempts
- **RBAC** - Administrator and viewer roles; viewer cannot access admin CRUD endpoints
- **Session management** - PostgreSQL-backed sessions with 1-day inactivity expiry; sessions invalidated on password change, email change, or MFA toggle via composite hash (BLAKE2b-512)
- **Rate limiting** - Two-tier system: general limiter (30 req/min) and stricter auth limiter (5 req/min) with configurable block durations
- **Request screening** - Blocks known attack patterns (PHP/WordPress/JNDI probes, scanner user agents) before they consume rate limit tokens
- **CSRF protection** - Token validation on all state-changing endpoints
- **Access logging** - Full audit trail with IP, user agent, action type, and outcome; automatic cleanup with configurable retention
- **Encryption at rest** - AES-256-GCM for TOTP secrets, verification tokens, and password reset tokens; key validated at startup
- **Input validation** - Strict validation on all forms: email format, password strength, alphanumeric access codes, filename sanitization
- **Prometheus metrics** - System and application metrics at `/metrics` (restricted to localhost, rejects proxied requests)

## Project Structure

```
├── src/
│   ├── main.rs              # Entry point, server setup, background tasks
│   ├── app.rs               # AppState, router construction, access code serving
│   ├── lib.rs               # Library root
│   ├── database.rs          # Database connection management
│   ├── settings.rs          # Settings service (DB-backed with env fallbacks)
│   ├── errors.rs            # AppError types with HTTP status mapping
│   ├── crypto.rs            # AES-256-GCM encryption/decryption
│   ├── oidc.rs              # OIDC/Keycloak client configuration
│   ├── email.rs             # Email service via AWS SES
│   ├── s3.rs                # S3-compatible file storage
│   ├── contact.rs           # Contact form handler
│   ├── subscribe.rs         # Newsletter subscription handler
│   ├── docx.rs              # DOCX template processing
│   ├── metrics.rs           # Prometheus metrics collection
│   ├── security_callbacks.rs # Rate limit callbacks and access logging
│   ├── admin/
│   │   ├── mod.rs           # Module exports, shared constants
│   │   ├── auth.rs          # Auth backend (create, verify, password, MFA)
│   │   ├── routes.rs        # Admin API endpoints (login, register, MFA, etc.)
│   │   ├── oidc_routes.rs   # OIDC login/callback handlers
│   │   ├── access_codes.rs  # Access code CRUD with S3 upload
│   │   ├── access_logs.rs   # Access log queries and dashboard metrics
│   │   ├── admin_users.rs   # Admin user management
│   │   ├── settings.rs      # Settings management endpoints
│   │   ├── totp.rs          # TOTP generation and verification
│   │   ├── password.rs      # Password validation rules
│   │   └── pagination.rs    # Pagination helpers
│   ├── entities/            # SeaORM entities (admin_user, access_code,
│   │                        #   access_log, setting, subscriber)
│   ├── middleware/
│   │   ├── mod.rs           # CSRF middleware wrapper
│   │   ├── admin_auth.rs    # Auth and role enforcement middleware
│   │   └── access_log.rs    # Access logging middleware
│   └── migration/           # 23 SeaORM migrations
├── admin-frontend/          # React admin SPA source
├── admin-assets/            # Built admin frontend
├── assets/                  # Static assets (icons, styles)
├── tests/                   # 16 integration test files + test helpers
│   └── common/              # Shared test utilities and mocks (SES, S3, OIDC)
├── Cargo.toml
├── Makefile
├── docker-compose.yml       # Development database
├── docker-compose.test.yml  # Test database
└── Dockerfile
```

## License

This project is dual-licensed under your choice of:

- [BSD-3-Clause](https://opensource.org/licenses/BSD-3-Clause)
- [GPL-3.0-only](https://www.gnu.org/licenses/gpl-3.0.html)
