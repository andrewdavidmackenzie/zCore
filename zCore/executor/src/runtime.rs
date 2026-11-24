use crate::{executor::Executor, task_collection::*};

#[cfg(target_arch = "x86_64")]
use crate::context::Context;
#[cfg(not(target_arch = "x86_64"))]
use crate::context::ContextData as Context;

use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
use core::{future::Future, pin::Pin};
use spin::{Mutex, MutexGuard};

pub struct ExecutorRuntime {
    // runtime only run on this cpu
    cpu_id: u8,

    // Runs on a single core only; no synchronization needed.
    task_collection: Arc<TaskCollection>,

    // force_switch_future demotes the strong_executor to a weak_executor.
    strong_executor: Arc<Pin<Box<Executor>>>,

    // Dropped after executing one future.
    weak_executors: Vec<Option<Arc<Pin<Box<Executor>>>>>,

    // The currently running executor.
    current_executor: Option<Arc<Pin<Box<Executor>>>>,

    // runtime context, WARN: riscv and x86_64 use different struct
    context: Context,
}

impl ExecutorRuntime {
    pub fn new(cpu_id: u8) -> Self {
        let task_collection = TaskCollection::new(cpu_id);
        let tc_clone = task_collection.clone();
        ExecutorRuntime {
            cpu_id,
            task_collection,
            strong_executor: Arc::new(Executor::new(tc_clone)),
            weak_executors: vec![],
            current_executor: None,
            context: Context::default(),
        }
    }

    pub fn cpu_id(&self) -> u8 {
        self.cpu_id
    }

    pub(crate) fn weak_executor_num(&self) -> usize {
        self.weak_executors.len()
    }

    // return task number of current cpu.
    pub fn task_num(&self) -> usize {
        self.task_collection.task_num()
    }

    fn add_weak_executor(&mut self, weak_executor: Arc<Pin<Box<Executor>>>) {
        self.weak_executors.push(Some(weak_executor));
    }

    fn downgrade_strong_executor(&mut self) {
        let mut old = self.strong_executor.clone();
        // SAFETY: downgrade_strong_executor is called from
        // run_until_idle, which runs with interrupts disabled
        // on a single CPU. No other code path can access this
        // Arc clone concurrently. mark_weak() sets an AtomicBool
        // on the Executor, which does not invalidate any
        // references — it only affects future scheduling decisions.
        unsafe {
            Arc::get_mut_unchecked(&mut old).mark_weak();
        }
        self.add_weak_executor(old);
        self.strong_executor = Arc::new(Executor::new(self.task_collection.clone()));
    }

    // Add a task. Its initial state is notified, meaning it is ready to execute.
    fn add_task<F: Future<Output = ()> + 'static + Send>(
        &self,
        priority: usize,
        future: F,
    ) -> (Key, usize) {
        debug_assert!(priority < MAX_PRIORITY);
        self.task_collection.priority_add_task(priority, future)
    }

    fn remove_task(&self, key: Key) {
        self.task_collection.remove_task(key)
    }

    fn get_context(&self) -> usize {
        #[cfg(target_arch = "x86_64")]
        {
            self.context.get_context()
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            &self.context as *const Context as usize
        }
    }
}

impl Drop for ExecutorRuntime {
    fn drop(&mut self) {
        panic!("drop executor runtime!!!!");
    }
}

// SAFETY: runs on a single core only; no synchronization needed
unsafe impl Send for ExecutorRuntime {}
unsafe impl Sync for ExecutorRuntime {}

/// Per-CPU executor runtimes, keyed by CPU ID.
/// Uses a spin::Once<HashMap> populated during SMP boot.
static CPU_RUNTIMES: spin::Once<hashbrown::HashMap<u8, Mutex<ExecutorRuntime>>> = spin::Once::new();

/// Per-CPU task collections, accessible without the runtime lock.
/// Used by work stealing to avoid deadlock: the runtime lock and
/// sched_state lock must never be held simultaneously by different
/// CPUs (the timer handler on the source CPU holds sched_state via
/// the executor, while trying to acquire the runtime lock in
/// sched_yield).
/// #862 diagnostics: total force_wake_task invocations (kill sweep /
/// migration fallback). Compare with per-task wake counts at hang time.
pub static FORCE_WAKE_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

static CPU_TASK_COLLECTIONS: spin::Once<hashbrown::HashMap<u8, Arc<TaskCollection>>> =
    spin::Once::new();

/// Global shutdown request, set by the kernel's shutdown path (the
/// `run_executor` wrapper once its future completes). Checked by
/// `run_until_idle`'s loop so the caller regains control even while
/// this CPU still has parked tasks: the loop otherwise spins inside
/// the executor's idle hlt until every task is gone, and a task whose
/// wake-up was lost (an issue #834 zombie) parks forever, hanging
/// shutdown.
static SHUTDOWN_REQUESTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Request that all CPUs' `run_until_idle` loops return to their
/// callers. Called by `run_executor` when its wrapped future
/// completes (the init process exited — the machine is shutting
/// down).
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, core::sync::atomic::Ordering::Release);
}

/// Whether shutdown has been requested.
pub fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(core::sync::atomic::Ordering::Acquire)
}

/// Initialize executor runtimes for a set of CPU IDs.
/// Called once by the BSP during boot, before any AP starts using the executor.
pub fn init_runtimes(cpu_ids: &[u8]) {
    CPU_RUNTIMES.call_once(|| {
        let mut map = hashbrown::HashMap::with_capacity(cpu_ids.len());
        for &id in cpu_ids {
            map.insert(id, Mutex::new(ExecutorRuntime::new(id)));
        }
        map
    });
    CPU_TASK_COLLECTIONS.call_once(|| {
        let runtimes = CPU_RUNTIMES.get().unwrap();
        let mut map = hashbrown::HashMap::with_capacity(cpu_ids.len());
        for &id in cpu_ids {
            let rt = runtimes.get(&id).unwrap().lock();
            map.insert(id, rt.task_collection.clone());
        }
        map
    });
}

fn get_runtime(cpu_id: u8) -> &'static Mutex<ExecutorRuntime> {
    CPU_RUNTIMES
        .get()
        .expect("executor runtimes not initialized")
        .get(&cpu_id)
        .unwrap_or_else(|| panic!("no runtime for cpu_id {}", cpu_id))
}

/// Steal one runnable task from another CPU directly into the local
/// CPU's collection.
///
/// The whole migration happens atomically under the source's
/// scheduler lock (see [`TaskCollection::steal_into`]) — there is no
/// transit window in which the task is unreachable by either CPU's
/// scanner, so wake-ups cannot be lost during migration.
///
/// Only steals from CPUs with > 1 task to avoid starving them.
/// Accesses task collections via `CPU_TASK_COLLECTIONS` (not via the
/// runtime lock) to prevent deadlock with the timer handler.
///
/// Returns the stolen task's id.
///
/// NOTE: currently unused — the executor's idle path keeps work
/// stealing disabled pending the wake-loss race tracked in the
/// follow-up to #783. Kept compiled (and exercised by future
/// re-enablement) with the rest of the Phase 6 infrastructure.
#[allow(dead_code)]
pub(crate) fn steal_task_from_other_cpu_into(dest: &Arc<TaskCollection>) -> Option<usize> {
    let tc_map = CPU_TASK_COLLECTIONS.get()?;
    let current = crate::arch::cpu_id();
    for (&cpu_id, source_tc) in tc_map.iter() {
        if cpu_id == current {
            continue;
        }
        if source_tc.task_num() <= 1 {
            continue;
        }
        if let Some(task_id) = source_tc.steal_into(dest) {
            return Some(task_id);
        }
    }
    None
}

/// Force-notify a task's waker slot, re-firing a potentially lost
/// wake-up. Safe to call from any CPU.
///
/// Used at process exit so shutdown does not depend on every kill
/// wake-up having been delivered (see the exit sweep in
/// `Process::exit`).
///
/// The saved `(cpu_id, key)` from `spawn_with_priority` can go
/// stale when work stealing migrates the task to another CPU (the
/// old slot is cleared and recycled). The task id disambiguates:
/// if the slot no longer holds that task, every collection is
/// searched for the id and the task is woken where it lives now.
/// A spurious re-notify is harmless: the task is simply polled
/// once more and re-parks if its wait is not satisfied.
pub fn force_wake_task(cpu_id: u8, key: usize, task_id: usize) {
    FORCE_WAKE_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let Some(map) = CPU_TASK_COLLECTIONS.get() else {
        return;
    };
    let Some(tc) = map.get(&cpu_id) else {
        return;
    };
    if tc.force_notify_verified(key, task_id) {
        return;
    }
    // Stale location (e.g. the task was migrated and the slot
    // recycled): find the task by id and wake it where it is now.
    for (_, other) in map.iter() {
        if other.force_notify_by_id(task_id) {
            return;
        }
    }
}

/// Get the local CPU's task collection for accepting stolen tasks.
pub(crate) fn local_task_collection() -> Option<&'static Arc<TaskCollection>> {
    let tc_map = CPU_TASK_COLLECTIONS.get()?;
    tc_map.get(&crate::arch::cpu_id())
}
// per-cpu scheduler.
pub fn run_until_idle() -> bool {
    debug!("run_until_idle");
    loop {
        // Disable interrupts before locking the runtime. The timer
        // interrupt handler calls sched_yield → get_current_runtime,
        // which locks the same runtime. Holding the runtime lock
        // with interrupts enabled causes a self-deadlock when the
        // timer fires on this CPU.
        crate::arch::intr_off();
        let mut runtime = get_current_runtime();
        let runtime_cx = runtime.get_context();
        let executor_cx = runtime.strong_executor.context.get_context();
        debug!("switch idle -> {}", runtime.strong_executor.id());
        runtime.current_executor = Some(runtime.strong_executor.clone());
        // Release the runtime lock before switching. Interrupts
        // stay disabled across the switch to prevent sched_yield
        // from re-entering the runtime context.
        drop(runtime);
        crate::cpu_stats(crate::arch::cpu_id() as usize)
            .context_switches
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        switch(runtime_cx, executor_cx);
        // Interrupts stay disabled from the intr_off at loop top.
        // Re-acquire runtime lock (still under intr_off).
        runtime = get_current_runtime();
        runtime.current_executor = None;
        if runtime.task_num() == 0 {
            drop(runtime);
            crate::arch::intr_on();
            return false;
        }
        // When shutdown is requested, return to the caller even if
        // tasks remain: the caller (run_executor) is shutting the
        // machine down and must not wait for parked tasks whose
        // wake-ups may never arrive.
        if shutdown_requested() {
            drop(runtime);
            crate::arch::intr_on();
            return true;
        }
        // When the strong executor was preempted mid-poll (holding
        // kernel locks, etc.), demote it to a weak executor and create
        // a fresh strong executor. Fall through to run all weak
        // executors so the preempted one can finish and release locks.
        if runtime.strong_executor.is_running_future() {
            crate::cpu_stats(crate::arch::cpu_id() as usize)
                .preempts
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            runtime.downgrade_strong_executor();
        }
        // Run all pending weak executors. This is critical for
        // preemption correctness: a demoted executor may hold a
        // kernel lock (e.g., the COW tree mutex). If we don't
        // resume it, every subsequent task that tries to acquire
        // the same lock will also block, creating an ever-growing
        // pile of demoted executors — a livelock.
        runtime
            .weak_executors
            .retain(|executor| executor.is_some() && !executor.as_ref().unwrap().killed());
        if !runtime.weak_executors.is_empty() {
            debug!("run weak executor");
            for idx in 0..runtime.weak_executors.len() {
                if let Some(executor) = &runtime.weak_executors[idx] {
                    if executor.killed() {
                        continue;
                    }
                    let executor = executor.clone();
                    let executor_ctx = executor.context.get_context();
                    debug!("switch idle -> {}", executor.id());
                    runtime.current_executor = Some(executor);
                    // Already under intr_off from loop top.
                    drop(runtime);
                    switch(runtime_cx as _, executor_ctx as _);
                    // Back from weak executor. Re-acquire under intr_off.
                    runtime = get_current_runtime();
                    runtime.current_executor = None;
                }
            }
        }
        drop(runtime);
        crate::arch::intr_on();
    }
}

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    let enable = crate::arch::intr_get();
    if enable {
        crate::arch::intr_off();
    }
    let cpu_id = crate::arch::cpu_id();
    let _ = spawn_task(future, None, Some(cpu_id));
    if enable {
        crate::arch::intr_on();
    }
}

/// Spawn a future with a specific priority, distributed across CPUs.
///
/// Uses the least-loaded CPU for load-balancing when multiple CPUs
/// are available. Falls back to current CPU if the executor hasn't
/// been initialized yet (early boot).
///
/// Returns `(task_key, cpu_id, task_id)` so the caller can later
/// change the task's priority via `change_task_priority` and
/// force-wake it via `force_wake_task` (the task id makes the wake
/// robust to work-stealing migration).
pub fn spawn_with_priority(
    future: impl Future<Output = ()> + Send + 'static,
    priority: usize,
) -> (Key, u8, usize) {
    let enable = crate::arch::intr_get();
    if enable {
        crate::arch::intr_off();
    }
    // Distribute tasks across CPUs on x86_64. On aarch64/riscv64,
    // hardware interrupts (serial, timer) are routed to CPU 0 and
    // the waker system doesn't propagate cross-CPU (#684). Tasks
    // on secondary CPUs can't be woken by IRQs on the BSP.
    // x86_64 uses polled serial I/O so this isn't an issue there.
    let cpu = if cfg!(target_arch = "x86_64") && CPU_RUNTIMES.get().is_some() {
        None
    } else {
        Some(crate::arch::cpu_id())
    };
    let result = spawn_task(future, Some(priority), cpu);
    if enable {
        crate::arch::intr_on();
    }
    result
}

/// Spawn a coroutine with `priority` on a specific CPU (by ID).
/// Default priority: DEFAULT_PRIORITY
/// Default CPU: the one with fewest tasks
///
/// Returns `(task_key, cpu_id, task_id)`.
pub fn spawn_task(
    future: impl Future<Output = ()> + Send + 'static,
    priority: Option<usize>,
    cpu_id: Option<u8>,
) -> (Key, u8, usize) {
    debug!("try to spawn {:?} cpu_id={:?}", priority, cpu_id);
    let priority = priority.unwrap_or(DEFAULT_PRIORITY);
    // Disable interrupts while holding runtime locks. The timer
    // interrupt handler calls sched_yield → get_current_runtime,
    // which locks the runtime. If we hold a runtime lock with
    // interrupts enabled, a timer on this CPU deadlocks on the
    // same lock.
    let enable = crate::arch::intr_get();
    if enable {
        crate::arch::intr_off();
    }
    let (runtime, actual_cpu) = if let Some(id) = cpu_id {
        (get_runtime(id), id)
    } else {
        let map = CPU_RUNTIMES.get().expect("executor not initialized");
        let current = crate::arch::cpu_id();
        let current_rt = map.get(&current);
        let current_tasks = current_rt.map(|r| r.lock().task_num()).unwrap_or(0);
        // Only distribute to other CPUs if the current CPU already
        // has tasks. This ensures the first spawned task (init process)
        // runs on the BSP, and subsequent tasks get distributed.
        if current_tasks == 0 {
            (current_rt.expect("current CPU not in runtime map"), current)
        } else {
            let (best_cpu, best_rt) = map
                .iter()
                .min_by_key(|(_, r)| r.lock().task_num())
                .expect("no CPUs registered");
            (best_rt, *best_cpu)
        }
    };
    let (key, task_id) = runtime.lock().add_task(priority, future);
    if enable {
        crate::arch::intr_on();
    }
    // If the task was spawned on a different CPU, send an IPI to
    // wake it from HLT/WFI so it sees the new task immediately.
    // Without this, a halted CPU won't poll the task until its
    // next timer interrupt (~10ms), causing WaitForBlocked timeouts.
    crate::ipi_wakeup(actual_cpu);
    (key, actual_cpu, task_id)
}

/// Change the scheduling priority of a task identified by its key on
/// a specific CPU. The task stays in the same FutureCollection slab
/// (the key's priority bits become stale) but the scheduler's internal
/// priority mapping and ready queue placement are updated.
///
/// This is safe to call from any CPU — it acquires the target CPU's
/// runtime lock with interrupts disabled.
pub fn change_task_priority(cpu_id: u8, key: Key, new_priority: usize) {
    let enable = crate::arch::intr_get();
    if enable {
        crate::arch::intr_off();
    }
    let runtime = get_runtime(cpu_id);
    runtime
        .lock()
        .task_collection
        .change_task_priority(key, new_priority);
    if enable {
        crate::arch::intr_on();
    }
}

/// check whether the running coroutine of current cpu time out, if yes, we will
/// switch to currrent cpu runtime that would create a new executor to run other
/// coroutines.
pub fn handle_timeout() {
    debug!("handle kernel timeout");
    let cpu = crate::arch::cpu_id() as usize;
    crate::cpu_stats(cpu)
        .reschedules
        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    super::run_with_intr_saved_off! {
        sched_yield()
    }
    // #862: after the preemption switch returns, the preempted task
    // resumes here — still inside the timer IRQ frame (the iretq that
    // ends the interrupt is further up the stack) — and continues its
    // kernel work below that frame. Re-enable interrupts for the
    // remainder of that execution: the EOI was already sent by
    // handle_irq (the APIC driver sends it before dispatching), so no
    // in-service IRQ is pending; nested interrupts push below the
    // current rsp and return normally; and the final iretq restores
    // the interrupted task's own rflags (the context switch now
    // saves/restores rflags, so the suspended context resumes with
    // its own interrupt state).
    crate::arch::intr_on();
}

/// Run executor.run()
#[no_mangle]
pub(crate) fn run_executor(executor_addr: usize) {
    let mut p = unsafe { Box::from_raw(executor_addr as *mut Executor) };
    p.run();
    // Weak executor may return. Disable interrupts before locking
    // the runtime to prevent handle_timeout → sched_yield from
    // deadlocking on the same lock.
    crate::arch::intr_off();
    let runtime = get_current_runtime();
    let executor_cx = p.context.get_context();
    let runtime_cx = runtime.get_context();
    debug!("executor all done! switch {} -> idle", p.id());
    drop(runtime);
    switch(executor_cx as _, runtime_cx as _);
    unreachable!();
}

/// switch to runtime, which would select an appropriate executor to run.
pub fn sched_yield() {
    let runtime = get_current_runtime();
    if let Some(executor) = runtime.current_executor.as_ref() {
        let executor_cx = executor.context.get_context();
        debug!("switch {} -> idle", executor.id());
        let runtime_cx = runtime.get_context();
        drop(runtime);
        switch(executor_cx, runtime_cx);
    }
}

/// Abandon the current poll and switch to the runtime idle context
/// WITHOUT saving the faulted context. Never returns on success.
///
/// For the kernel-trap fatal path (#523): a kernel-mode fault inside a
/// task's poll can neither be resumed (the faulting instruction would
/// re-execute forever) nor unwound (locks may be held). Halting the
/// CPU loses it entirely; `sched_yield` would save the faulted context
/// and the demoted-weak-executor machinery would resume it later,
/// re-faulting in an infinite loop. Instead: discard the context. The
/// faulted task is orphaned — its future is never re-polled, so the
/// `Arc<Thread>` it holds leaks (an accepted graceful-degradation
/// tradeoff, bounded by the number of kernel faults) — and the
/// executor's stack is reused by subsequent polls, clobbering the
/// abandoned frames.
///
/// The abandoned executor is left mid-poll (`task_id != 0`), so when
/// the runtime next returns from `switch` it takes the existing
/// preemption path: demotes it to a weak executor and later resumes it
/// at its last legitimate suspension point (saved by a prior
/// `sched_yield`), after which the normal weak-executor completion
/// path (WEAK → KILLED → dropped → stack freed) reclaims it.
///
/// # Safety
///
/// Must be called from interrupt context with interrupts disabled
/// (the runtime-lock discipline — see `run_until_idle`). Returns
/// `false` without switching when the runtime lock is held by the
/// interrupted (faulted) code, when no executor is running a poll, or
/// on architectures without the no-save switch; the caller must halt
/// the CPU in that case.
pub fn abandon_current_poll() -> bool {
    // Lock discipline: the runtime lock is only ever held with
    // interrupts disabled. We are in interrupt context, so any
    // contention means the lock is held either by another CPU (about
    // to release — but we cannot yield to wait) or, worse, by the
    // interrupted task on this CPU (which can never release it — its
    // poll is about to be abandoned). try_lock, never spin.
    if crate::arch::intr_get() {
        return false; // must run with interrupts disabled
    }
    #[cfg(target_arch = "x86_64")]
    {
        let Some(runtime) = get_runtime(crate::arch::cpu_id()).try_lock() else {
            return false;
        };
        // Borrow through the guard instead of cloning: a clone would
        // leak (this function never returns past the switch) and keep
        // the executor's Arc refcount elevated, breaking
        // `Arc::get_mut_unchecked`'s uniqueness assumption in
        // `downgrade_strong_executor` and leaking the 512 KiB stack.
        let running = runtime
            .current_executor
            .as_ref()
            .is_some_and(|executor| executor.is_running_future());
        if !running {
            return false; // not mid-poll — nothing to abandon
        }
        let runtime_cx = runtime.get_context();
        drop(runtime);
        switch_no_save(runtime_cx as _);
        unreachable!()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub(crate) fn switch(from_ctx: usize, to_ctx: usize) {
    unsafe {
        crate::arch::switch(from_ctx as _, to_ctx as _);
    }
}

/// Load the context at `to_ctx` and resume it, discarding the current
/// context. x86_64 only (declared in arch/x86_64/mod.rs).
#[cfg(target_arch = "x86_64")]
pub(crate) fn switch_no_save(to_ctx: usize) {
    unsafe {
        crate::arch::switch_no_save(to_ctx as _);
    }
}

/// return runtime `MutexGuard` of current cpu.
pub(crate) fn get_current_runtime() -> MutexGuard<'static, ExecutorRuntime> {
    get_runtime(crate::arch::cpu_id()).lock()
}

#[allow(dead_code)]
// Just for debug
pub fn get_current_executor_id() -> (usize, usize) {
    let runtime = get_current_runtime();
    if let Some(executor) = runtime.current_executor.as_ref() {
        (executor.id(), executor.task_id())
    } else {
        (0, 0)
    }
}
