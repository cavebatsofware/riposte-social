#!/bin/ksh
# Builder VM post-install provisioning.
#
# Runs from /etc/rc.firsttime on the autoinstall'd OpenBSD VM. Installs
# the rust toolchain and the supporting tools needed for
# `cargo build --release`. Frontend assets are produced on the host;
# this image deliberately does not install bun or node.
set -eu

PATH=/sbin:/usr/sbin:/bin:/usr/bin:/usr/local/sbin:/usr/local/bin
export PATH

# Tell pkg_add to prefer the release branch matching the running kernel.
PKG_PATH=https://cdn.openbsd.org/pub/OpenBSD/$(uname -r)/packages/$(uname -m)/
export PKG_PATH

# Refuse interactive prompts; the autoinstall flow has no console.
pkg_add -I -z \
  rust \
  git \
  gmake \
  pkgconf

# The builder mounts the repo via 9p / virtfs at /repo when scripts/run-in-builder.sh
# launches it. Make sure that mountpoint exists.
mkdir -p /repo

# Create the build user. The repo bind-mount is owned by the host so we
# can't chown it; cargo just needs a writable HOME for its caches and
# target/ directory.
useradd -m -s /bin/ksh build
echo "permit nopass keepenv build" > /etc/doas.conf
chmod 0440 /etc/doas.conf

# Cargo lives in ~build; cache the registry and git checkouts there so a
# rebuild from a fresh repo bind-mount doesn't redownload crates.
mkdir -p /home/build/.cargo
cat > /home/build/.cargo/config.toml <<'EOF'
[build]
# Default target lives in the bind-mounted repo so the produced binary
# is available to the host once the build finishes.
target-dir = "/repo/target"
EOF
chown -R build:build /home/build/.cargo

echo "builder provisioning complete; shut down with 'shutdown -hp now'"
