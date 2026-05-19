#!/usr/bin/env bash
# Launch the builder VM with the repo bind-mounted at /repo and run
# `cargo build --release` inside. Exits when the build finishes; the
# resulting binary lands in $REPO/target/release/riposte-social on the
# host.
#
# Args:
#   $1  builder qcow2 image
#   $2  repo root on the host
set -euo pipefail

if [[ $# -ne 2 ]]; then
	echo "usage: $0 <builder.qcow2> <repo-root>" >&2
	exit 64
fi

BUILDER=$1
REPO=$2

# 9p virtfs share: the builder mounts this as /repo and cargo writes
# back through it. security_model=mapped-xattr keeps host-side perms
# sensible since we're crossing a UID boundary.
#
# Use KVM if available; cargo's parallel codegen pegs every vCPU we
# give it, so KVM acceleration is the difference between minutes and
# tens of minutes.
KVM_FLAG=()
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
	KVM_FLAG+=(-enable-kvm)
fi
qemu-system-x86_64 \
	"${KVM_FLAG[@]}" \
	-m 6144 -smp 4 \
	-drive file="$BUILDER",format=qcow2,if=virtio \
	-virtfs local,path="$REPO",mount_tag=repo,security_model=mapped-xattr,id=repo \
	-netdev user,id=net0 \
	-device virtio-net,netdev=net0 \
	-nographic \
	-snapshot \
	-no-reboot

echo "build complete; binary at $REPO/target/release/riposte-social"
