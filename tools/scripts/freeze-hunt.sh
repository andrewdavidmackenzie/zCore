#!/bin/zsh
# freeze-hunt.sh — #862: catch the total kernel freeze and photograph it.
#
# The freeze: all vCPUs stop with interrupts disabled (serial + timer dead).
# Two independent dump paths, because a frozen guest can starve QEMU's
# main-loop-serviced interfaces:
#   1. lldb attached EARLY (at boot, while healthy) in async mode, then
#      `process interrupt` at freeze time — full symbolized backtraces.
#   2. QEMU HMP monitor socket: `stop` + per-CPU `info registers` — raw
#      RIPs, symbolized offline with llvm-symbolizer.
#
# Usage: zsh tools/scripts/freeze-hunt.sh

KERN=target/core-tests.qemu-x86_64/release/kernel
LOG=/tmp/freeze-hunt.log
LLDB_LOG=/tmp/freeze-hunt-lldb.log
MON_DUMP=/tmp/freeze-hunt-mon.txt
SYM=/tmp/freeze-hunt-syms.txt
FIFO=/tmp/freeze-hunt-fifo
MON=/tmp/freeze-hunt-mon.sock
GDB_PORT=1234
SILENCE_SECS=90        # serial silence that means "frozen"
MIN_TESTS=50           # tests that must pass before silence counts as a freeze
MAX_WAIT=1800          # give up after 30 min

rm -f $LOG $LLDB_LOG $MON_DUMP $SYM $FIFO $MON

. tools/scripts/find-ovmf.sh && OVMF=$(find_ovmf)
if [ -z "$OVMF" ]; then echo "OVMF not found"; exit 1; fi

qemu-system-x86_64 -m 4G -display none -no-reboot \
    -machine q35 -smp 8 \
    -cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
    -serial file:$LOG \
    -monitor unix:$MON,server,nowait \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive format=raw,file=target/core-tests.qemu-x86_64/release/boot.img \
    -gdb tcp:127.0.0.1:$GDB_PORT &
QPID=$!
echo "QEMU pid=$QPID, gdb :$GDB_PORT, monitor $MON, serial -> $LOG"

# Attach lldb early (handshake while the guest is healthy), async mode so
# `process continue` returns and the fifo keeps accepting commands.
mkfifo $FIFO
lldb "$KERN" < $FIFO > $LLDB_LOG 2>&1 &
LLDB_PID=$!
exec 3<>$FIFO
echo "gdb-remote 127.0.0.1:$GDB_PORT" >&3
sleep 8
echo "settings set target.process.async true" >&3
echo "process continue" >&3
echo "=== lldb attached, guest running ==="

LAST_SIZE=0
SILENT=0
ELAPSED=0
FROZEN=0
LAST_TESTS=0
NOPROGRESS=0
while kill -0 $QPID 2>/dev/null; do
    sleep 10
    ELAPSED=$((ELAPSED + 10))
    if [ $ELAPSED -ge $MAX_WAIT ]; then
        echo "TIMEOUT: no freeze in ${MAX_WAIT}s"
        break
    fi
    SIZE=$(stat -f%z $LOG 2>/dev/null); SIZE=${SIZE:-0}
    OK_COUNT=$(grep -c '\[       OK \]' $LOG 2>/dev/null); OK_COUNT=${OK_COUNT:-0}
    if [ "$OK_COUNT" -lt $MIN_TESTS ]; then
        LAST_SIZE=$SIZE
        continue
    fi
    if [ "$SIZE" = "$LAST_SIZE" ]; then
        SILENT=$((SILENT + 10))
    else
        SILENT=0
        LAST_SIZE=$SIZE
    fi
    # No-test-progress: the silent hang mode keeps producing serial
    # output (executor scans) while the suite is stuck — serial
    # silence alone won't catch it.
    if [ "$OK_COUNT" = "$LAST_TESTS" ]; then
        NOPROGRESS=$((NOPROGRESS + 10))
    else
        NOPROGRESS=0
        LAST_TESTS=$OK_COUNT
    fi
    printf 'elapsed=%ss tests=%d silent=%ss noprogress=%ss\n' $ELAPSED $OK_COUNT $SILENT $NOPROGRESS
    if [ $SILENT -ge $SILENCE_SECS ]; then
        echo "FREEZE DETECTED: ${SILENCE_SECS}s serial silence at $OK_COUNT tests"
        FROZEN=1
        break
    fi
    if [ $NOPROGRESS -ge 300 ]; then
        echo "HANG DETECTED: 300s with no test progress (at $OK_COUNT tests, serial still active — silent mode)"
        FROZEN=1
        break
    fi
done

if [ $FROZEN -eq 1 ]; then
    echo "=== last 10 serial lines before freeze ==="
    tail -10 $LOG | sed 's/\x1b\[[0-9;]*m//g'

    # Halt the VM via the QEMU monitor FIRST — the gdbstub's halt
    # times out on a live guest (vCPUs must cooperate), but the
    # monitor's `stop` halts from the main loop.
    (echo stop; sleep 3; echo quit) | nc -U $MON > /dev/null 2>&1
    sleep 2

    # Path 1: lldb attaches to the now-stopped VM.
    echo "settings set auto-confirm true" >&3
    echo "process interrupt" >&3
    sleep 5
    echo "thread list" >&3
    sleep 3
    for N in 1 2 3 4 5 6 7 8; do
        echo "thread select $N" >&3
        echo "register read rip rsp rbp" >&3
        echo "bt 12" >&3
        echo "memory read -s 8 -f x -c 128 \$rsp" >&3
    done
    sleep 25
    echo "detach" >&3
    echo "quit" >&3
    sleep 3

    # Path 2: HMP monitor per-CPU registers (raw RIPs)
    {
        for N in 0 1 2 3 4 5 6 7; do
            echo "cpu $N"
            sleep 1
            echo 'info registers'
            sleep 2
        done
        echo quit
    } | nc -U $MON > $MON_DUMP 2>&1

    # Symbolize the HMP RIPs offline
    grep -oE 'RIP=[0-9a-f]+' $MON_DUMP | cut -d= -f2 | sort -u | while read rip; do
        echo "== $rip:" >> $SYM
        printf '0x%s\n' "$rip" | xargs /opt/homebrew/opt/llvm/bin/llvm-symbolizer --obj=$KERN --functions=linkage --inlines 2>/dev/null >> $SYM
    done

    echo "=== lldb dump (tail) ==="
    tail -30 $LLDB_LOG
    echo "=== monitor dump (registers head) ==="
    head -40 $MON_DUMP
    echo "=== symbolized RIPs ==="
    cat $SYM 2>/dev/null
else
    echo "no freeze captured"
fi

echo "quit" >&3 2>/dev/null
kill -9 $LLDB_PID 2>/dev/null
kill -9 $QPID 2>/dev/null
exec 3>&-
echo "FREEZE-HUNT DONE frozen=$FROZEN"
