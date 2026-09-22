use alloc::{string::String, sync::Arc};
use zircon_object::{object::KernelObject, task::Process};

#[derive(Debug)]
pub struct BootOptions {
    #[allow(dead_code)]
    pub cmdline: String,
    /// Root process path (e.g. "/bin/busybox?sh" or "/bin/hello").
    pub root_proc: String,
}

pub fn boot_options() -> BootOptions {
    use alloc::string::ToString;
    let cmdline = hal_impl::boot::cmdline();
    let root_proc = parse_cmdline_value(&cmdline, "ROOTPROC")
        .unwrap_or(if cfg!(feature = "linux") {
            "/bin/busybox?sh"
        } else {
            "/bin/hello"
        })
        .to_string();
    BootOptions { cmdline, root_proc }
}

/// Determine which personality to boot from command line or defaults.
///
/// Checks `PERSONALITY=linux` or `PERSONALITY=zircon` in the command line.
/// Falls back to the compile-time default: linux if enabled, else zircon.
pub fn parse_personality(cmdline: &str) -> &'static str {
    if let Some(p) = parse_cmdline_value(cmdline, "PERSONALITY") {
        match p {
            #[cfg(feature = "linux")]
            "linux" => return "linux",
            #[cfg(feature = "zircon")]
            "zircon" => return "zircon",
            _ => warn!(
                "PERSONALITY={} not available (not compiled or unknown), using default",
                p
            ),
        }
    }
    // Default: prefer linux if compiled in, else zircon
    if cfg!(feature = "linux") {
        "linux"
    } else {
        "zircon"
    }
}

/// Extract a value from a "KEY=VALUE KEY2=VALUE2" cmdline string.
fn parse_cmdline_value<'a>(cmdline: &'a str, key: &str) -> Option<&'a str> {
    for token in cmdline.split_whitespace() {
        let mut iter = token.splitn(2, '=');
        if let (Some(k), Some(v)) = (iter.next(), iter.next()) {
            if k.trim() == key {
                return Some(v.trim());
            }
        }
    }
    None
}

/// Wait for the init process to exit, then terminate.
pub fn wait_for_exit(proc: Option<Arc<Process>>) -> ! {
    let exit_code = if let Some(proc) = proc {
        let future = async move {
            use zircon_object::object::Signal;
            let object: Arc<dyn KernelObject> = proc.clone();
            // Wait for either termination signal — Linux processes use
            // PROCESS_TERMINATED, Zircon processes use USER_SIGNAL_0.
            // In dual-personality mode, wait for either.
            let signal = Signal::PROCESS_TERMINATED | Signal::USER_SIGNAL_0;
            object.wait_signal(signal).await;
            check_exit_code(proc)
        };
        hal_impl::run_executor(future)
    } else {
        // Secondary core: enter the executor idle loop to service
        // tasks spawned by the primary core. The future never
        // completes — secondary cores run until the system shuts down.
        let future = core::future::pending::<i32>();
        hal_impl::run_executor(future)
    };
    info!("exiting with code {}", exit_code);
    hal_impl::cpu::reset()
}

fn check_exit_code(proc: Arc<Process>) -> i32 {
    let code = proc.exit_code().unwrap_or(-1);
    if code != 0 {
        error!(
            "process {:?}({}) exited with code {:?}",
            proc.name(),
            proc.id(),
            code
        );
    } else {
        info!(
            "process {:?}({}) exited with code 0",
            proc.name(),
            proc.id()
        )
    }
    code as i32
}
