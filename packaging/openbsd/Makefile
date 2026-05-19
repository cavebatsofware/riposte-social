# OpenBSD VM build pipeline for riposte-social.
#
# Two-stage shape mirroring the top-level Dockerfile:
#   1. builder.qcow2  OpenBSD-current with rust toolchain. Cached across
#                     rebuilds; cargo's target/ persists inside.
#   2. runner.qcow2   Minimal OpenBSD with the compiled binary, the SPA
#                     assets, an rc.d script, and a service user. No
#                     toolchain. This is the deployable artifact.
#
# Frontend assets are built on the host with `bun run build` (run from
# the repo root) before invoking the runner stage; the resulting
# social-assets/ and admin-assets/ are copied into the runner image
# without ever entering OpenBSD's userland.
#
# Requirements on the host:
#   - qemu-system-x86_64 OR vmm/vmctl (Linux uses qemu; OpenBSD uses vmm)
#   - curl (to fetch the OpenBSD installer)
#   - signify-openbsd (to verify installer signatures)
#   - genisoimage / mkisofs (to author the autoinstall response disk)
#
# Network: the build VMs use user-mode networking (qemu -net user) so no
# host configuration is needed; pkg_add and rustup pulls happen through
# NAT.

OPENBSD_VERSION ?= 7.9
OPENBSD_MIRROR  ?= https://cdn.openbsd.org/pub/OpenBSD
INSTALLER_ISO   := install$(subst .,,$(OPENBSD_VERSION)).iso
INSTALLER_URL   := $(OPENBSD_MIRROR)/$(OPENBSD_VERSION)/amd64/$(INSTALLER_ISO)

# signify is named `signify-openbsd` on Debian/Ubuntu and just `signify`
# on OpenBSD and NixOS. Prefer the suffixed name if present.
SIGNIFY := $(shell command -v signify-openbsd 2>/dev/null || command -v signify 2>/dev/null)

CACHE_DIR       := cache
BUILD_DIR       := build
BUILDER_IMG     := $(BUILD_DIR)/builder.qcow2
RUNNER_IMG      := $(BUILD_DIR)/runner.qcow2
BUILDER_SIZE    := 12G
RUNNER_SIZE     := 4G

# Ephemeral SSH key used to provision the builder over ssh after the
# base install finishes. Lives under build/ so `make clean` removes
# it; never check it into the repo.
SSH_KEY         := $(BUILD_DIR)/build_ed25519
SSH_PUB         := $(SSH_KEY).pub

# Repo-root paths used when wiring the runner image. Treated as build
# inputs; the runner target depends on them existing.
REPO_ROOT       := $(abspath ../..)
RELEASE_BINARY  := $(REPO_ROOT)/target/x86_64-unknown-openbsd/release/riposte-social
SOCIAL_ASSETS   := $(REPO_ROOT)/social-assets
ADMIN_ASSETS    := $(REPO_ROOT)/admin-assets

.PHONY: all builder runner clean distclean fetch verify help

all: runner

help:
	@echo "OpenBSD VM build targets:"
	@echo "  make fetch     download the OpenBSD installer ISO"
	@echo "  make builder   build the builder VM (rust toolchain)"
	@echo "  make build-bin run cargo build inside the builder; emits riposte-social"
	@echo "  make runner    build the runner VM (binary + assets + rc.d)"
	@echo "  make boot      boot the runner under qemu and exec into it"
	@echo "  make clean     remove build artifacts (keep cached installer)"
	@echo "  make distclean remove build artifacts and cache"

$(CACHE_DIR) $(BUILD_DIR):
	mkdir -p $@

# Fetch and verify the OpenBSD installer ISO.
fetch: $(CACHE_DIR)/$(INSTALLER_ISO) $(CACHE_DIR)/SHA256.sig

$(CACHE_DIR)/$(INSTALLER_ISO): | $(CACHE_DIR)
	curl -fL -o $@ $(INSTALLER_URL)

$(CACHE_DIR)/SHA256.sig: | $(CACHE_DIR)
	curl -fL -o $@ $(OPENBSD_MIRROR)/$(OPENBSD_VERSION)/amd64/SHA256.sig

# Fetch the signing key into the cache so verification is self-
# contained; Linux/NixOS don't ship OpenBSD's signify pubkeys.
$(CACHE_DIR)/openbsd-$(subst .,,$(OPENBSD_VERSION))-base.pub: | $(CACHE_DIR)
	curl -fL -o $@ $(OPENBSD_MIRROR)/$(OPENBSD_VERSION)/openbsd-$(subst .,,$(OPENBSD_VERSION))-base.pub

verify: fetch $(CACHE_DIR)/openbsd-$(subst .,,$(OPENBSD_VERSION))-base.pub
	@if [ -z "$(SIGNIFY)" ]; then \
		echo "error: signify not found on PATH (looked for signify-openbsd, signify)" >&2; \
		exit 1; \
	fi
	cd $(CACHE_DIR) && $(SIGNIFY) -C -p openbsd-$(subst .,,$(OPENBSD_VERSION))-base.pub -x SHA256.sig $(INSTALLER_ISO)

# Generate the SSH keypair used to log in to the builder for
# provisioning. ed25519 because it's small and supported by every
# OpenBSD-shipped sshd. ssh-keygen writes both files in one invocation;
# the .pub target piggybacks on the private-key rule.
$(SSH_KEY): | $(BUILD_DIR)
	ssh-keygen -t ed25519 -N '' -C 'riposte-social builder' -f $@ </dev/null

$(SSH_PUB): $(SSH_KEY)
	@touch $@

# Builder image: OpenBSD-current + rust + git. Two-stage build:
#   1. Run the autoinstall sequence to lay down the base OS, with the
#      ephemeral SSH pubkey injected into install.conf so root has it
#      ready on first boot.
#   2. Boot the installed disk, ssh in over a forwarded port, and run
#      provision-builder.sh to install rust + git + the build user.
builder: $(BUILDER_IMG)

$(BUILDER_IMG): fetch auto_install.conf provision-builder.sh $(SSH_PUB) | $(BUILD_DIR)
	qemu-img create -f qcow2 $@ $(BUILDER_SIZE)
	./scripts/run-autoinstall.sh $@ $(CACHE_DIR)/$(INSTALLER_ISO) auto_install.conf $(SSH_PUB)
	./scripts/provision-via-ssh.sh $@ $(SSH_KEY) provision-builder.sh

# Compile the release binary inside the builder VM. Mounts the repo
# read-write so cargo emits the binary back to the host filesystem.
build-bin: $(BUILDER_IMG)
	./scripts/run-in-builder.sh $(BUILDER_IMG) $(REPO_ROOT)

# Runner image: depends on the binary and the host-built SPA assets.
runner: $(RUNNER_IMG)

$(RUNNER_IMG): $(RELEASE_BINARY) $(SOCIAL_ASSETS) $(ADMIN_ASSETS) provision-runner.sh riposte_social.rc | $(BUILD_DIR)
	qemu-img create -f qcow2 $@ $(RUNNER_SIZE)
	./scripts/run-autoinstall.sh $@ $(CACHE_DIR)/$(INSTALLER_ISO) auto_install.conf provision-runner.sh

$(RELEASE_BINARY):
	$(MAKE) build-bin

$(SOCIAL_ASSETS) $(ADMIN_ASSETS):
	@echo "Run 'bun run build' from the repo root to produce $@" >&2
	@exit 1

# Boot the runner in foreground for smoke testing. Forwards 3000 -> 3000.
boot: $(RUNNER_IMG)
	qemu-system-x86_64 -accel kvm:tcg -m 2048 -smp 2 \
	  -drive file=$(RUNNER_IMG),format=qcow2 \
	  -object rng-random,filename=/dev/urandom,id=rng0 \
	  -device virtio-rng-pci,rng=rng0 \
	  -netdev user,id=net0,hostfwd=tcp::3000-:3000 \
	  -device virtio-net,netdev=net0 \
	  -nographic

clean:
	rm -rf $(BUILD_DIR)

distclean: clean
	rm -rf $(CACHE_DIR)
