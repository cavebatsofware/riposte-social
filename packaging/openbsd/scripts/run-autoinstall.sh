#!/usr/bin/env bash
# Boot the OpenBSD installer in qemu and feed it the autoinstall
# response over HTTP. OpenBSD 7.9's autoinstall(8) fetches its config
# file via HTTP; qemu user-mode NAT exposes the host to the guest as
# 10.0.2.2, so we serve install.conf from a temporary directory via a
# backgrounded python http.server and point the installer at it.
#
# Post-install provisioning is handled separately via SSH (see
# scripts/provision-via-ssh.sh). This script just lays down the base
# OS with our ephemeral SSH pubkey injected so the follow-up SSH step
# can log in as root.
#
# Args:
#   $1  output qcow2 disk image (already created with qemu-img)
#   $2  installer ISO path (cache/installXX.iso)
#   $3  autoinstall response file (auto_install.conf)
#   $4  path to the ssh pubkey to inject into root's authorized_keys
set -euo pipefail

if [[ $# -ne 4 ]]; then
	echo "usage: $0 <disk.qcow2> <installer.iso> <auto_install.conf> <ssh_pubkey>" >&2
	exit 64
fi

DISK=$1
ISO=$2
AUTO=$3
PUBKEY=$4

WORK=$(mktemp -d)
HTTP_PID=
cleanup() {
	if [[ -n "$HTTP_PID" ]] && kill -0 "$HTTP_PID" 2>/dev/null; then
		kill "$HTTP_PID" 2>/dev/null || true
		wait "$HTTP_PID" 2>/dev/null || true
	fi
	rm -rf "$WORK"
}
trap cleanup EXIT

# Stage install.conf with the ssh pubkey substituted in. The
# auto_install.conf checked into the repo carries a placeholder line
# `Public ssh key for root account = no`; we rewrite it to the actual
# pubkey contents so root has SSH access on first boot.
mkdir -p "$WORK/www"
PUBKEY_CONTENT=$(<"$PUBKEY")
sed -e "s|^Public ssh key for root account =.*|Public ssh key for root account = $PUBKEY_CONTENT|" \
    "$AUTO" > "$WORK/www/install.conf"

# Start an HTTP server on a high port. The guest reaches the host at
# 10.0.2.2 (qemu user-mode NAT), so the response URL becomes
# http://10.0.2.2:8989/install.conf. Bind to all interfaces so the
# guest's NAT mapping resolves.
HTTP_PORT=8989
python3 -m http.server --directory "$WORK/www" --bind 0.0.0.0 "$HTTP_PORT" \
	> "$WORK/http.log" 2>&1 &
HTTP_PID=$!
# Give the server a moment to bind before we boot the guest.
for _ in 1 2 3 4 5; do
	if curl -fsS "http://127.0.0.1:$HTTP_PORT/install.conf" >/dev/null 2>&1; then
		break
	fi
	sleep 0.2
done

DIR=$(cd -- "$(dirname -- "$0")" && pwd)

# Use KVM acceleration if /dev/kvm is present; fall back to software
# emulation otherwise (e.g. OpenBSD vmm hosts where qemu has no KVM
# backend). KVM is roughly 10-20x faster than TCG for both the install
# and the later cargo build.
KVM_FLAG=()
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
	KVM_FLAG+=(-enable-kvm)
fi

# Drive qemu via expect so we can interrupt cdboot's 5-second auto-boot
# timer, set the console to com0, boot bsd.rd, choose Autoinstall, and
# answer the response-URL prompt.
"$DIR/autoinstall.exp" \
	"${KVM_FLAG[@]}" \
	-m 2048 -smp 4 \
	-drive file="$DISK",format=qcow2,if=virtio \
	-object rng-random,filename=/dev/urandom,id=rng0 \
	-device virtio-rng-pci,rng=rng0 \
	-cdrom "$ISO" \
	-boot d \
	-netdev user,id=net0 \
	-device virtio-net,netdev=net0 \
	-nographic \
	-no-reboot
