// petal hello world -- placeholder for future #![no_std] implementation
//
// When petal programs can be cross-compiled and loaded from a ZBI,
// this will become a #![no_std] #![no_main] binary using zircon-abi:
//
//   #![no_std]
//   #![no_main]
//
//   use zircon_abi::syscall::{zx_debug_write, zx_process_exit};
//
//   #[no_mangle]
//   pub extern "C" fn _start() -> ! {
//       let msg = b"Hello from petal!\n";
//       unsafe { zx_debug_write(msg.as_ptr(), msg.len()); }
//       unsafe { zx_process_exit(0); }
//   }
//
// For now, the kernel generates an equivalent hello program as machine
// code and loads it directly. See loader/src/zircon.rs userstart_code().

fn main() {
    // This program cannot currently link (requires Fuchsia sysroot).
    // It exists as documentation of the intended petal program structure.
    println!("Hello from petal!");
}
