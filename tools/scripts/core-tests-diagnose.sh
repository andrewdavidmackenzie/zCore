#!/bin/bash
# Diagnose the post-summary hang: after grace, query QEMU monitor state,
# sample IRQ counts, inject NMIs to all CPUs, and check whether the
# kernel revives (new heartbeat lines in the log).
#
# Usage: core-tests-diagnose.sh <LOGFILE> <GRACE_SECONDS> <MON_SOCK> [CORES]
set -u
LOG=${1:?log}
GRACE=${2:?grace}
MON=${3:?monitor socket}
CORES=${4:-8}
BOOT_IMG=target/core-tests.qemu-x86_64/release/boot.img

pkill -f 'qemu-system-x86_64.*boot\.img' 2>/dev/null
sleep 1
rm -f "$MON"

. "$(dirname "$0")/find-ovmf.sh"
OVMF=$(find_ovmf) || { echo "ERROR: no OVMF found"; exit 1; }

echo "==> Running QEMU (log=$LOG, monitor=$MON, grace=${GRACE}s)..."
qemu-system-x86_64 -m 4G -display none -no-reboot -nographic \
    -machine q35 -smp "$CORES" \
    -cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
    -serial mon:stdio \
    -monitor unix:"$MON",server,nowait \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive "format=raw,file=$BOOT_IMG" \
    > "$LOG" 2>&1 &
PID=$!

START=$(date +%s)
SUMMARY_SEEN=0
while true; do
    if ! kill -0 $PID 2>/dev/null; then
        echo "==> QEMU exited on its own"
        break
    fi
    if [ $SUMMARY_SEEN -eq 0 ] && grep -q '\[==========\].*ran' "$LOG" 2>/dev/null; then
        echo "==> Test summary detected, waiting ${GRACE}s..."
        SUMMARY_SEEN=1
        SUMMARY_AT=$(date +%s)
    fi
    NOW=$(date +%s)
    if [ $SUMMARY_SEEN -eq 1 ] && [ $((NOW - SUMMARY_AT)) -ge $GRACE ]; then
        echo "==> Grace over; QEMU alive — running diagnostics"
        HB_BEFORE=$(tr -d '\000' < "$LOG" | grep -c "scan: cpu=" || true)
        python3 - "$MON" "$CORES" <<'EOF'
import socket, sys, time
mon, cores = sys.argv[1], int(sys.argv[2])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(mon)
s.settimeout(0.5)
def cmd(c, wait=0.4):
    s.sendall((c + "\n").encode())
    time.sleep(wait)
    out = b""
    try:
        while True:
            d = s.recv(65536)
            if not d:
                break
            out += d
    except socket.timeout:
        pass
    return out.decode(errors="replace")
cmd("", 0.2)
print("===== info status ====="); print(cmd("info status"))
print("===== info irq (sample 1) ====="); print(cmd("info irq"))
time.sleep(5)
print("===== info irq (sample 2, 5s later) ====="); print(cmd("info irq"))
print("===== injecting NMIs to all cpus =====")
for n in range(cores):
    print(cmd(f"nmi {n}"))
print("===== info registers cpu 0 (post-NMI) =====")
print(cmd("cpu 0")); print(cmd("info registers"))
EOF
        echo "==> Waiting 90s to see if kernel revives..."
        sleep 90
        HB_AFTER=$(tr -d '\000' < "$LOG" | grep -c "scan: cpu=" || true)
        echo "heartbeats before=$HB_BEFORE after=$HB_AFTER"
        if [ "$HB_AFTER" -gt "$HB_BEFORE" ]; then
            echo "==> KERNEL REVIVED BY NMI — CPUs were halt-stuck, interrupt sources dead"
            echo "===== registers after revival (cpu 0) ====="
            python3 - "$MON" <<'EOF'
import socket, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1]); s.settimeout(0.5)
def cmd(c, wait=0.4):
    s.sendall((c + "\n").encode()); time.sleep(wait)
    out = b""
    try:
        while True:
            d = s.recv(65536)
            if not d: break
            out += d
    except socket.timeout: pass
    return out.decode(errors="replace")
print(cmd("cpu 0")); print(cmd("info registers"))
EOF
        else
            echo "==> NMI did NOT revive kernel — likely QEMU-suspended (ACPI sleep)"
        fi
        echo "===== Killing QEMU ====="
        kill $PID 2>/dev/null
        wait $PID 2>/dev/null
        break
    fi
    if [ $((NOW - START)) -ge 1800 ]; then
        echo "==> Hard timeout, killing"
        kill $PID 2>/dev/null; wait $PID 2>/dev/null
        break
    fi
    sleep 5
done

P=$(grep -c '\[       OK \]' "$LOG" 2>/dev/null || true)
echo "=== Results: ${P:-0} passed ==="
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep "INIT EXIT\|exiting with code" | tail -3
echo "=== Last heartbeats ==="
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep "scan: cpu=" | tail -10
