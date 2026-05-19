# OpenBSD VM build

Self-contained pipeline that produces a minimal OpenBSD VM image
carrying just the `riposte-social` binary and its prebuilt SPA assets.
Mirrors the multi-stage shape of the top-level `Dockerfile`:

```
host (Linux/OpenBSD)            builder.qcow2              runner.qcow2
+--------------------+         +-----------------+        +------------------+
|  bun run build     |         | OpenBSD-current |        | OpenBSD minimal  |
|    -> social-      | -----> | + rust + git    | -----> | + riposte-social |
|       assets/      |        |                 |        | + assets/        |
|    -> admin-       |        | cargo build     |        | + rc.d script    |
|       assets/      |        | --release       |        | + service user   |
+--------------------+        +-----------------+        +------------------+
                                     |                          |
                                     v                          v
                              target/release/             pushed to ECR via
                              riposte-social              ORAS as OCI artifact
```

The runner does **not** contain a toolchain, a database, or paradedb.
The app talks to whatever sits at `DATABASE_URL`; operators choose
whether that's paradedb, stock postgres, or a managed service.

## Status

Exploratory. The pipeline is scaffolded but has not been run end-to-end
on this host yet. Known open items:

- Rust on OpenBSD: should work via `pkg_add rust`, but the crate graph
  hasn't been built there yet. The two likely failure modes are (a) a
  crate with a hard C dep that misses an OpenBSD library path and
  (b) a feature-gate that compiles a Linux-only syscall. The
  `e2e_testing` feature is the most exposed; the production build is
  safer.
- `bun` is intentionally absent from the builder; SPA assets are
  produced on the host and copied in. `bun` does not have an OpenBSD
  port at time of writing.
- The autoinstall response disk is built with `genisoimage`; OpenBSD
  hosts use `mkisofs` from `cdrtools`. Substitute as needed.

## Prerequisites

Host (Linux):

```
qemu-system-x86_64
signify-openbsd
genisoimage
curl
```

Host (OpenBSD): `vmm`/`vmctl` works too; the `qemu-system-x86_64`
invocations in `scripts/` translate directly. Patches welcome.

## End-to-end build

```sh
# 1. From the repo root, produce the SPA assets.
bun run build

# 2. From packaging/openbsd/, fetch the installer and build the
#    builder VM. This step is slow the first time (full OpenBSD
#    install + pkg_add rust) and cached afterwards.
cd packaging/openbsd
make builder

# 3. Build the release binary inside the builder. The repo is bind-
#    mounted into the VM; the binary lands on the host at
#    target/release/riposte-social.
make build-bin

# 4. Compose the runner image: copies the binary and the SPA assets
#    into a fresh OpenBSD install, writes the rc.d script, and enables
#    the service.
make runner

# 5. Smoke test: boot the runner and curl /healthz.
make boot
# in another shell:
curl -fsS http://localhost:3000/healthz
```

## What the runner contains

- `/var/riposte_social/riposte-social` (binary, owned `riposte_social:riposte_social`)
- `/var/riposte_social/social-assets/` (SPA)
- `/var/riposte_social/admin-assets/` (SPA)
- `/etc/rc.d/riposte_social` (service script)
- `/etc/riposte_social.env` (placeholder; operators replace)
- `riposte_social` service user (locked password, `/sbin/nologin`)

The runner runs zero `pkg_add` calls. Everything else it needs is in
base OpenBSD: TLS roots at `/etc/ssl/cert.pem` (the binary is rustls-
only), `rc.subr`, `ksh`. No toolchain, no bun/node/rust, no build
headers, no curl. The image is base userland + this app.

Out of the box the service tries to connect to
`postgres://riposte_social_user:CHANGE_ME@localhost:5432/...`; replace
`/etc/riposte_social.env` with real values before starting it. The
smoke-test `make boot` flow assumes you have a database reachable from
the VM's user-mode network namespace (typically the host's
docker-compose paradedb container at `10.0.2.2:5432`).

## Files

| Path | What it is |
| --- | --- |
| `Makefile` | Top-level build driver. |
| `auto_install.conf` | OpenBSD autoinstall(8) response file. |
| `provision-builder.sh` | Post-install setup for the builder VM. |
| `provision-runner.sh` | Post-install setup for the runner VM. |
| `riposte_social.rc` | rc.d(8) service script. |
| `scripts/run-autoinstall.sh` | Boot the installer + response disk under qemu. |
| `scripts/run-in-builder.sh` | Re-enter the builder VM to run cargo build. |

## ECR push

Not wired up here; producing the qcow2 is the boundary of this
directory. The existing OCI/ECR conversion path (used today for the
NixOS images) consumes `build/runner.qcow2` as input and handles the
ORAS push, signing, and registry mirroring.
