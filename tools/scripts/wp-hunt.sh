#!/bin/zsh
# wp-hunt.sh — #903 Phase 2: run the (already filtered) watchpoint test
# under QEMU's gdb stub, wait for the wedge, dump all vCPU stacks.
# The boot.img must already have the gtest filter baked in.

KERN=target/core-tests.qemu-x86_64/release/kernel
LOG=/tmp/wp-hunt.log
LLDB_LOG=/tmp/wp-hunt-lldb.log
FIFO=/tmp/wp-hunt-fifo
GDB_PORT=1234
WEDGE_WAIT=${WEDGE_WAIT:-300}   # seconds to let the wedge form

rm -f $LOG $LLDB_LOG $FIFO

. tools/scripts/find-ovmf.sh && OVMF=$(find_ovmf)
if [ -z "$OVMF" ]; then echo "OVMF not found"; exit 1; fi

qemu-system-x86_64 -m 4G -display none -no-reboot \
    -machine q35 -smp 8 \
    -cpu qemu64,+rdrand,+rdtscp,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cx16 \
    -serial file:$LOG \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive format=raw,file=target/core-tests.qemu-x86_64/release/boot.img \
    -gdb tcp:127.0.0.1:$GDB_PORT &
QPID=$!
echo "QEMU pid=$QPID gdb :$GDB_PORT serial -> $LOG"

# Attach lldb early (handshake while healthy), async so continue returns.
mkfifo $FIFO
lldb "$KERN" < $FIFO > $LLDB_LOG 2>&1 &
LLDB_PID=$!
exec 3<>$FIFO
echo "settings set auto-confirm true" >&3
echo "gdb-remote 127.0.0.1:$GDB_PORT" >&3
sleep 8
echo "process continue" >&3
echo "=== lldb attached, guest running; waiting ${WEDGE_WAIT}s for the wedge ==="

sleep $WEDGE_WAIT
echo "=== last serial lines ==="
tail -4 $LOG | sed 's/\x1b\[[0-9;]*m//g'

# The guest is wedged (cpu=7 frozen since ~1s; others halted) — haltable.
echo "process interrupt" >&3
sleep 8
echo "thread list" >&3
sleep 3
for N in 1 2 3 4 5 6 7 8; do
    echo "thread select $N" >&3
    echo "register read rip rsp rbp" >&3
    echo "bt 15" >&3
done
sleep 25
echo "detach" >&3
echo "quit" >&3
sleep 3

echo "=== lldb session ==="
cat $LLDB_LOG

kill -9 $LLDB_PID $QPID 2>/dev/null
exec 3>&-
echo "WP-HUNT DONE"
