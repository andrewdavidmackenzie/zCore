import re
import socket
import sys
import time

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
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
# Enumerate the guest CPUs from `info cpus` instead of assuming a count.
info = cmd("info cpus")
cpus = sorted(int(m) for m in re.findall(r"CPU #(\d+)", info))
if not cpus:
    cpus = [0]

print("== info lapic (cpu0) ==")
print(cmd("info lapic"))

# x86 LAPIC MMIO offsets (relative to 0xFEE00000):
#   0x0F0 spurious-interrupt vector register (bit 8 = APIC software enable)
#   0x320 LVT timer register
#   0x380 timer initial-count register (TICR)
#   0x390 timer current-count register (TCCR)
#   0x380 ICR is written via 0x300/0x310 on xAPIC; ICR low is at 0x300.
REGS = [
    ("SPIV (0xF0)", 0x0F0),
    ("LVT timer (0x320)", 0x320),
    ("timer initial count (0x380)", 0x380),
    ("timer current count (0x390)", 0x390),
    ("ICR low (0x300)", 0x300),
]

for n in cpus:
    cmd("cpu %d" % n)
    print("== LAPIC MMIO cpu%d ==" % n)
    for name, off in REGS:
        print("  %s: %s" % (name, cmd("xp /1gx 0x%x" % (0xFEE00000 + off)).strip().splitlines()[-1]))
