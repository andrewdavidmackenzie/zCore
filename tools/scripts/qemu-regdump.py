import re
import socket
import sys
import time

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
s.settimeout(0.2)


def cmd(c, deadline=3.0):
    """Send an HMP command and read until the `(qemu)` prompt.

    Reading until the prompt (rather than a fixed sleep) keeps
    responses synchronized; the overall deadline bounds a stalled
    monitor so the script fails loudly instead of silently
    misattributing partial output.
    """
    s.sendall((c + "\n").encode())
    out = b""
    end = time.time() + deadline
    while time.time() < end:
        try:
            d = s.recv(65536)
            if not d:
                break
            out += d
            if b"(qemu)" in out:
                break
        except socket.timeout:
            continue
    if b"(qemu)" not in out:
        sys.stderr.write("warning: no HMP prompt for %r (partial output)\n" % c)
    return out.decode(errors="replace")


cmd("", 1.0)
# Enumerate the guest CPUs from `info cpus` instead of assuming a count.
info = cmd("info cpus")
cpus = sorted(int(m) for m in re.findall(r"CPU #(\d+)", info))
if not cpus:
    sys.stderr.write("error: could not enumerate CPUs from `info cpus`\n")
    sys.exit(1)

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
