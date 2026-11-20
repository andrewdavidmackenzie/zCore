#!/usr/bin/env python3
"""Symbolize kernel addresses from a QEMU monitor dump.

Reads lines containing kernel text addresses (bare hex after ``RIP=``
or ``RSP=``-style register dumps, or ``0x``-prefixed values) from
stdin or a file, and symbolizes each against the zCore kernel ELF
using addr2line.

Usage: symbolize.py <kernel-elf> [dump-file]
"""
import re
import subprocess
import sys
from collections import OrderedDict

KERNEL = sys.argv[1]
SRC = open(sys.argv[2]) if len(sys.argv) > 2 else sys.stdin

# Collect kernel text addresses in order of appearance, deduplicated.
# Accept both `0xffffffff8...` and bare hex following `RIP=`/`RSP=`.
addrs = OrderedDict()
for line in SRC:
    for m in re.finditer(r"0x(ffffffff8[0-9a-f]+)", line, re.IGNORECASE):
        addrs.setdefault("0x" + m.group(1).lower(), None)
    for m in re.finditer(r"R(?:IP|SP)=(ffffffff8[0-9a-f]+)", line, re.IGNORECASE):
        addrs.setdefault("0x" + m.group(1).lower(), None)

if not addrs:
    print("no kernel addresses found")
    sys.exit(0)

addrs = list(addrs.keys())
proc = subprocess.run(
    ["addr2line", "-e", KERNEL, "-f", "-C", "-a", *addrs],
    capture_output=True,
    text=True,
)
if proc.returncode != 0:
    sys.stderr.write("addr2line failed (exit %d): %s\n" % (proc.returncode, proc.stderr))
    sys.exit(proc.returncode)

# addr2line -a prints "0xADDR\nfunc\nfile:line" triples
lines = proc.stdout.splitlines()
i = 0
while i + 2 < len(lines) + 1:
    try:
        addr, func, loc = lines[i], lines[i + 1], lines[i + 2]
    except IndexError:
        break
    print(f"{addr}  {func}  @ {loc}")
    i += 3
