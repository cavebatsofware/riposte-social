#!/usr/bin/env bash
# Boot a freshly-installed builder image and run the provision script
# inside it over SSH, then halt the VM cleanly.
#
# qemu is launched with -daemonize + a Unix monitor socket so the
# script doesn't sit on the foreground; the VM is shut down via
# `shutdown -hp now` over ssh, and qemu exits when the guest halts.
#
# Args:
#   $1  disk image (builder.qcow2)
#   $2  SSH private key
#   $3  provision script to run inside the VM (e.g. provision-builder.sh)
set -euo pipefail

if [[ $# -ne 3 ]]; then
	echo "usage: $0 <disk.qcow2> <ssh_key> <provision.sh>" >&2
	exit 64
fi

DISK=$1
KEY=$2
PROVISION=$3

WORK=$(mktemp -d)
QEMU_PID_FILE=$WORK/qemu.pid
MONITOR_SOCK=$WORK/qemu.mon
SSH_PORT=2222

cleanup() {
	if [[ -f "$QEMU_PID_FILE" ]] && kill -0 "$(<"$QEMU_PID_FILE")" 2>/dev/null; then
		kill "$(<"$QEMU_PID_FILE")" 2>/dev/null || true
	fi
	rm -rf "$WORK"
}
trap cleanup EXIT

KVM_FLAG=()
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
	KVM_FLAG+=(-enable-kvm)
fi

echo "[provision-via-ssh] booting $DISK..."
# virtio-rng is critical: without a fast entropy source the freshly-
# installed guest has no random.seed and sshd hangs for minutes on the
# first SSH handshake while waiting for /dev/random to seed.
qemu-system-x86_64 \
	"${KVM_FLAG[@]}" \
	-m 2048 -smp 4 \
	-drive file="$DISK",format=qcow2,if=virtio \
	-object rng-random,filename=/dev/urandom,id=rng0 \
	-device virtio-rng-pci,rng=rng0 \
	-netdev user,id=net0,hostfwd=tcp:127.0.0.1:$SSH_PORT-:22 \
	-device virtio-net,netdev=net0 \
	-display none \
	-serial null \
	-monitor unix:"$MONITOR_SOCK",server,nowait \
	-pidfile "$QEMU_PID_FILE" \
	-daemonize \
	-no-reboot

# Wait for sshd to accept connections. OpenBSD enables sshd by default;
# first boot brings the network up and starts sshd in under a minute.
echo "[provision-via-ssh] waiting for sshd on 127.0.0.1:$SSH_PORT..."
SSH_OPTS=(
	-i "$KEY"
	-p "$SSH_PORT"
	-o StrictHostKeyChecking=no
	-o UserKnownHostsFile=/dev/null
	-o LogLevel=ERROR
	-o ConnectTimeout=5
)
for attempt in $(seq 1 60); do
	if ssh "${SSH_OPTS[@]}" -o BatchMode=yes root@127.0.0.1 true 2>/dev/null; then
		echo "[provision-via-ssh] sshd up after ${attempt}s"
		break
	fi
	sleep 2
	if (( attempt == 60 )); then
		echo "[provision-via-ssh] sshd never came up; giving up" >&2
		exit 2
	fi
done

# Pipe the provision script into a remote shell. Running it via stdin
# avoids having to scp the script first, and any error inside the
# script propagates back via the ssh exit code thanks to `set -e`
# inside provision-builder.sh.
echo "[provision-via-ssh] running $(basename "$PROVISION") on the VM..."
ssh "${SSH_OPTS[@]}" root@127.0.0.1 'ksh -s' < "$PROVISION"

# Shut down cleanly. `shutdown -hp now` halts and powers off; with
# -no-reboot in the qemu args, the VM process exits.
echo "[provision-via-ssh] shutting down VM..."
ssh "${SSH_OPTS[@]}" root@127.0.0.1 'shutdown -hp now' || true

# Wait for qemu to exit (up to 60s).
PID=$(<"$QEMU_PID_FILE")
for _ in $(seq 1 60); do
	if ! kill -0 "$PID" 2>/dev/null; then
		echo "[provision-via-ssh] VM halted"
		exit 0
	fi
	sleep 1
done
echo "[provision-via-ssh] VM did not exit cleanly; forcing" >&2
kill "$PID" 2>/dev/null || true
exit 1
