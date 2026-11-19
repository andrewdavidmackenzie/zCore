import re
import socket
import sys
import time

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
s.settimeout(0.5)


def cmd(c, wait=0.35):
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

for n in cpus:
    cmd("cpu %d" % n)
    regs = cmd("info registers")
    rip = re.search(r"RIP=([0-9a-f]+)", regs)
    rfl = re.search(r"RFL=([0-9a-f]+)", regs)
    hlt = "HLT=1" in regs
    rsp = re.search(r"RSP=([0-9a-f]+)", regs)
    print("cpu%d: RIP=%s RSP=%s RFL=%s halted=%s" % (
        n,
        rip.group(1) if rip else "?",
        rsp.group(1) if rsp else "?",
        rfl.group(1) if rfl else "?",
        "yes" if hlt else "NO"))
