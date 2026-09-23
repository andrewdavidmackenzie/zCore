//! Minimal WASI runner for zCore.
//!
//! Reads a .wasm file from argv[1], provides WASI preview1 host functions
//! (fd_write for stdout/stderr, proc_exit), and runs it.
//!
//! Built as a musl-static Linux binary and placed in the rootfs.

use std::env;
use std::fs;
use std::process;
use wasmi::{Caller, Engine, Linker, Module, Store};

/// State passed through the wasmi Store.
struct WasiState {
    /// Exit code set by proc_exit.
    exit_code: Option<i32>,
    /// Program arguments (argv[0] = wasm path).
    args: Vec<String>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: wasi-runner <file.wasm>");
        process::exit(1);
    }

    let wasm_path = &args[1];
    let wasm_bytes = match fs::read(wasm_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("wasi-runner: cannot read '{}': {}", wasm_path, e);
            process::exit(1);
        }
    };

    let engine = Engine::default();
    let module = match Module::new(&engine, &wasm_bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("wasi-runner: invalid WASM: {}", e);
            process::exit(1);
        }
    };

    // Pass the wasm path as argv[0] to the guest.
    let guest_args = vec![wasm_path.clone()];
    let mut store = Store::new(
        &engine,
        WasiState {
            exit_code: None,
            args: guest_args,
        },
    );
    let mut linker = <Linker<WasiState>>::new(&engine);

    register_wasi(&mut linker);

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .unwrap_or_else(|e| {
            eprintln!("wasi-runner: instantiation failed: {}", e);
            process::exit(1);
        });

    let start = instance
        .get_typed_func::<(), ()>(&store, "_start")
        .unwrap_or_else(|e| {
            eprintln!("wasi-runner: _start not found: {}", e);
            process::exit(1);
        });

    match start.call(&mut store, ()) {
        Ok(()) => {}
        Err(_) => {
            // proc_exit triggers a trap — check if we have an exit code
            if store.data().exit_code.is_none() {
                eprintln!("wasi-runner: _start trapped");
                process::exit(1);
            }
        }
    }

    let code = store.data().exit_code.unwrap_or(0);
    process::exit(code);
}

fn register_wasi(linker: &mut Linker<WasiState>) {
    // fd_write(fd, iovs, iovs_len, nwritten) -> errno
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "fd_write",
            |mut caller: Caller<'_, WasiState>,
             fd: i32,
             iovs_ptr: i32,
             iovs_len: i32,
             nwritten_ptr: i32|
             -> i32 {
                if fd != 1 && fd != 2 {
                    return 8; // EBADF
                }
                let memory = match caller.get_export("memory") {
                    Some(wasmi::Extern::Memory(m)) => m,
                    _ => return 8,
                };
                let mem_data = memory.data(&caller);
                let mut total = 0u32;
                for i in 0..iovs_len {
                    let iov_off = (iovs_ptr as usize) + (i as usize) * 8;
                    if iov_off + 8 > mem_data.len() {
                        return 21; // EFAULT
                    }
                    let ptr = u32::from_le_bytes(
                        mem_data[iov_off..iov_off + 4].try_into().unwrap(),
                    ) as usize;
                    let len = u32::from_le_bytes(
                        mem_data[iov_off + 4..iov_off + 8].try_into().unwrap(),
                    ) as usize;
                    if ptr + len > mem_data.len() {
                        return 21;
                    }
                    // Write to host stdout/stderr
                    let buf = &mem_data[ptr..ptr + len];
                    if fd == 1 {
                        use std::io::Write;
                        let _ = std::io::stdout().write_all(buf);
                    } else {
                        use std::io::Write;
                        let _ = std::io::stderr().write_all(buf);
                    }
                    total += len as u32;
                }
                let mem_data = memory.data_mut(&mut caller);
                let nw = nwritten_ptr as usize;
                if nw + 4 <= mem_data.len() {
                    mem_data[nw..nw + 4].copy_from_slice(&total.to_le_bytes());
                }
                0 // success
            },
        )
        .expect("fd_write");

    // proc_exit(code)
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "proc_exit",
            |mut caller: Caller<'_, WasiState>, code: i32| {
                caller.data_mut().exit_code = Some(code);
            },
        )
        .expect("proc_exit");

    // args_sizes_get(argc_ptr, argv_buf_size_ptr) -> errno
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "args_sizes_get",
            |mut caller: Caller<'_, WasiState>, argc_ptr: i32, buf_size_ptr: i32| -> i32 {
                let argc = caller.data().args.len() as u32;
                let buf_size: u32 = caller.data().args.iter().map(|a| a.len() as u32 + 1).sum();
                let memory = match caller.get_export("memory") {
                    Some(wasmi::Extern::Memory(m)) => m,
                    _ => return 8,
                };
                let mem = memory.data_mut(&mut caller);
                let ap = argc_ptr as usize;
                let bp = buf_size_ptr as usize;
                if ap + 4 > mem.len() || bp + 4 > mem.len() {
                    return 21;
                }
                mem[ap..ap + 4].copy_from_slice(&argc.to_le_bytes());
                mem[bp..bp + 4].copy_from_slice(&buf_size.to_le_bytes());
                0
            },
        )
        .expect("args_sizes_get");

    // args_get(argv_ptr, argv_buf_ptr) -> errno
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "args_get",
            |mut caller: Caller<'_, WasiState>, argv_ptr: i32, argv_buf_ptr: i32| -> i32 {
                let args = caller.data().args.clone();
                let memory = match caller.get_export("memory") {
                    Some(wasmi::Extern::Memory(m)) => m,
                    _ => return 8,
                };
                let mem = memory.data_mut(&mut caller);
                let mut buf_offset = argv_buf_ptr as usize;
                for (i, arg) in args.iter().enumerate() {
                    let argv_off = (argv_ptr as usize) + i * 4;
                    if argv_off + 4 > mem.len() {
                        return 21;
                    }
                    mem[argv_off..argv_off + 4]
                        .copy_from_slice(&(buf_offset as u32).to_le_bytes());
                    let bytes = arg.as_bytes();
                    if buf_offset + bytes.len() + 1 > mem.len() {
                        return 21;
                    }
                    mem[buf_offset..buf_offset + bytes.len()].copy_from_slice(bytes);
                    mem[buf_offset + bytes.len()] = 0; // null terminator
                    buf_offset += bytes.len() + 1;
                }
                0
            },
        )
        .expect("args_get");

    // environ_sizes_get(count_ptr, buf_size_ptr) -> errno
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "environ_sizes_get",
            |mut caller: Caller<'_, WasiState>, count_ptr: i32, buf_size_ptr: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmi::Extern::Memory(m)) => m,
                    _ => return 8,
                };
                let mem = memory.data_mut(&mut caller);
                let cp = count_ptr as usize;
                let bp = buf_size_ptr as usize;
                if cp + 4 > mem.len() || bp + 4 > mem.len() {
                    return 21;
                }
                mem[cp..cp + 4].copy_from_slice(&0u32.to_le_bytes());
                mem[bp..bp + 4].copy_from_slice(&0u32.to_le_bytes());
                0
            },
        )
        .expect("environ_sizes_get");

    // environ_get(environ_ptr, environ_buf_ptr) -> errno
    linker
        .func_wrap(
            "wasi_snapshot_preview1",
            "environ_get",
            |_caller: Caller<'_, WasiState>, _environ_ptr: i32, _environ_buf_ptr: i32| -> i32 {
                0 // no env vars
            },
        )
        .expect("environ_get");
}
