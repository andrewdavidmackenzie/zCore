//! Thread spawning.

use alloc::sync::Arc;
use core::{any::Any, future::Future};

use crate::{config::MAX_CORE_NUM, utils::PerCpuCell};

#[allow(clippy::declare_interior_mutable_const)]
const DEFAULT_THREAD: PerCpuCell<Option<Arc<dyn Any + Send + Sync>>> = PerCpuCell::new(None);

static CURRENT_THREAD: [PerCpuCell<Option<Arc<dyn Any + Send + Sync>>>; MAX_CORE_NUM] =
    [DEFAULT_THREAD; MAX_CORE_NUM];

hal_fn_impl! {
    impl mod crate::hal_fn::thread {
        fn spawn(future: impl Future<Output = ()> + Send + 'static) {
            executor::spawn(future);
        }

        fn set_current_thread(thread: Option<Arc<dyn Any + Send + Sync>>) {
            let idx = super::cpu::cpu_index();
            *CURRENT_THREAD[idx].get_mut() = thread;
        }

        fn get_current_thread() -> Option<Arc<dyn Any + Send + Sync>> {
            let idx = super::cpu::cpu_index();
            if let Some(arc_thread) = CURRENT_THREAD[idx].get().as_ref() {
                Some(arc_thread.clone())
            } else {
                None
            }
        }
    }
}
