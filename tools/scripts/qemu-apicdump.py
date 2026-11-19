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
cmd("", 0.2)
print("== info lapic (cpu0) =="); print(cmd("info lapic"))
# LAPIC MMIO (xAPIC mode): timer regs + spurious + LVT timer
# 0xFEE000F0 spurious (bit8=APIC enable), 0xFEE00320 timer ICR,
# 0xFEE00328 timer current, 0xFEE00330 LVT timer, 0xFEE00380 ICR lo
print("== LAPIC MMIO cpu0 ==")
print(cmd("xp /1gx 0xfee000f0"))
print(cmd("xp /2gx 0xfee00320"))
print(cmd("xp /1gx 0xfee00330"))
print(cmd("xp /1gx 0xfee00380"))
# switch to cpu 1 and repeat spurious + timer
cmd("cpu 1")
print("== LAPIC MMIO cpu1 ==")
print(cmd("xp /1gx 0xfee000f0"))
print(cmd("xp /2gx 0xfee00320"))
print(cmd("xp /1gx 0xfee00330"))
