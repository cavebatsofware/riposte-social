#!/bin/ksh
# Runner VM post-install provisioning.
#
# Produces a minimal OpenBSD image carrying just the compiled
# riposte-social binary, the prebuilt SPA assets, an rc.d service
# script, and the service user. No toolchain, no compiler, no
# bun/node/rust. The database is external; this image only ships the
# app.
set -eu

PATH=/sbin:/usr/sbin:/bin:/usr/bin:/usr/local/sbin:/usr/local/bin
export PATH

# Runtime deps: none. The binary is rustls-only and OpenBSD base
# already ships /etc/ssl/cert.pem; the rc.d framework is in base; the
# SPA assets are static. Deliberately no pkg_add: no toolchain, no
# bun/node/rust, no build headers, no curl. The image is just the
# kernel + base userland + this app.

# Service account. Locked password; no shell login. Home holds the
# binary, the static assets, and a writable runtime directory for any
# session/cookie state the app keeps on disk.
useradd -m -d /var/riposte_social -s /sbin/nologin -L daemon riposte_social

# Files come in from the host via a one-shot 9p / virtfs mount attached
# by scripts/run-autoinstall.sh; they live under /payload during this
# provisioning run.
install -m 0755 -o riposte_social -g riposte_social \
  /payload/riposte-social /var/riposte_social/riposte-social

mkdir -p /var/riposte_social/social-assets /var/riposte_social/admin-assets
cp -R /payload/social-assets/. /var/riposte_social/social-assets/
cp -R /payload/admin-assets/. /var/riposte_social/admin-assets/
chown -R riposte_social:riposte_social /var/riposte_social

# rc.d service. Enables on next boot.
install -m 0555 /payload/riposte_social.rc /etc/rc.d/riposte_social
rcctl enable riposte_social

# Default environment file. Operators replace this on deploy with their
# actual DATABASE_URL and secrets; the values shipped here are
# placeholders that intentionally won't connect.
cat > /etc/riposte_social.env <<'EOF'
# Configuration for the riposte-social service.
#
# DATABASE_URL points at the operator's paradedb (or stock postgres)
# instance. The app itself doesn't care which one; FTS is a deployment
# choice.
DATABASE_URL=postgres://riposte_social_user:CHANGE_ME@localhost:5432/riposte_social

# Bind to all interfaces inside the VM. Operators front this with
# relayd / httpd for TLS termination on the host side.
BIND_HOST=0.0.0.0
PORT=3000

# Set to warn or error in production. The rc.d script defaults to info
# if this is unset.
RUST_LOG=info
EOF
chmod 0640 /etc/riposte_social.env
chown root:riposte_social /etc/riposte_social.env

echo "runner provisioning complete; shut down with 'shutdown -hp now'"
