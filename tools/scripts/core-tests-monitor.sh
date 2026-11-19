#!/bin/bash
# Run x86_64 core-tests QEMU with a monitor socket. After the gtest
# summary (+ grace), dump per-CPU registers and stacks via the QEMU
# monitor, then kill QEMU. Use to localize stuck executors.
#
# Usage: core-tests-monitor.sh <LOGFILE> <GRACE_SECONDS> <MON_SOCK> [CORES]
set -u
LOG=${1:?log}
GRACE=${2:?grace}
MON=${3:?monitor socket}
CORES=${4:-8}
BOOT_IMG=target/core-tests.qemu-x86_64/release/boot.img

# Do not pkill other sessions' QEMU; only manage our own child via $PID below.
sleep 1
rm -f "$MON"

. "$(dirname "$0")/find-ovmf.sh"
OVMF=$(find_ovmf) || { echo "ERROR: no OVMF found"; exit 1; }

echo "==> Running QEMU (log=$LOG, monitor=$MON, grace=${GRACE}s, cores=$CORES)..."
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
        echo "==> Grace over; QEMU still alive — dumping CPU state via monitor"
        python3 - "$MON" "$CORES" <<'EOF'
import socket, sys, time
mon, cores = sys.argv[1], int(sys.argv[2])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(mon)
s.settimeout(0.5)
buf = b""
def drain():
    global buf
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
def cmd(c, wait=0.4):
    s.sendall((c + "\n").encode())
    time.sleep(wait)
    return drain()
cmd("", 0.2)
print("===== info cpus =====")
print(cmd("info cpus"))
for n in range(cores):
    print(f"===== cpu {n} registers =====")
    print(cmd(f"cpu {n}"))
    print(cmd("info registers"))
    rsp = "?"
    # grab RSP from the register dump for a stack walk
    import re
    regs = cmd("info registers")
    m = re.search(r"RSP=([0-9a-f]+)", regs)
    print(regs)
    if m:
        rsp = m.group(1)
        print(f"===== cpu {n} stack @ 0x{rsp} =====")
        print(cmd(f"x/48gx 0x{rsp}"))
EOF
        echo "==> Killing QEMU"
        kill $PID 2>/dev/null
        wait $PID 2>/dev/null
        break
    fi
    if [ $((NOW - START)) -ge 1800 ]; then
        echo "==> Hard timeout, killing"
        kill $PID 2>/dev/null
        wait $PID 2>/dev/null
        break
    fi
    sleep 5
    if [ $(( ($(date +%s) - START) % 60 )) -lt 6 ]; then
        P=$(grep -c '\[       OK \]' "$LOG" 2>/dev/null || true)
        echo "    ${P:-0} tests passed ($(( $(date +%s) - START ))s)..."
    fi
done

P=$(grep -c '\[       OK \]' "$LOG" 2>/dev/null || true)
echo "=== Results: ${P:-0} passed ==="
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep "INIT EXIT\|exiting with code" | tail -3
echo "=== Last heartbeats ==="
tr -d '\000' < "$LOG" | sed 's/\x1b\[[0-9;]*m//g' | grep "scan: cpu=" | tail -8
