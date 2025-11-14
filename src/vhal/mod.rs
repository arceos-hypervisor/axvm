use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use axtask::AxCpuMask;

use crate::{
    TASK_STACK_SIZE,
    arch::{self, Hal},
};

mod timer;

pub fn init() -> anyhow::Result<()> {
    Hal::init();

    static CORES: AtomicUsize = AtomicUsize::new(0);

    let cpu_count = cpu_count();

    info!("Initializing VHal for {cpu_count} CPUs...");

    for cpu_id in 0..cpu_count {
        let _handle = axtask::spawn_raw(
            move || {
                info!("Core {cpu_id} is initializing hardware virtualization support...");
                // Initialize cpu affinity here.
                assert!(
                    axtask::set_current_affinity(AxCpuMask::one_shot(cpu_id)),
                    "Initialize CPU affinity failed!"
                );
                info!("Enabling hardware virtualization support on core {cpu_id}");
                timer::init_percpu();

                let _ = CORES.fetch_add(1, Ordering::Release);
            },
            format!("init-cpu-{}", cpu_id),
            TASK_STACK_SIZE,
        );
    }
    info!("Waiting for all cores to enable hardware virtualization...");

    // Wait for all cores to enable virtualization.
    while CORES.load(Ordering::Acquire) != cpu_count {
        // Use `yield_now` instead of `core::hint::spin_loop` to avoid deadlock.
        axtask::yield_now();
    }

    info!("All cores have enabled hardware virtualization support.");
    Ok(())
}

pub fn cpu_count() -> usize {
    axruntime::cpu_count()
}

pub(crate) trait ArchHal {
    fn init() -> anyhow::Result<()>;
    fn cpu_list() -> Vec<CpuHardId>;
    fn current_enable_viretualization() -> anyhow::Result<()>;
}

pub fn current_enable_viretualization() -> anyhow::Result<()> {
    Hal::current_enable_viretualization()
}

pub(crate) struct PreCpuSet<T>(UnsafeCell<BTreeMap<usize, Option<T>>>);

unsafe impl<T> Sync for PreCpuSet<T> {}
unsafe impl<T> Send for PreCpuSet<T> {}

impl<T> PreCpuSet<T> {
    pub const fn new() -> Self {
        PreCpuSet(UnsafeCell::new(BTreeMap::new()))
    }

    unsafe fn set(&self, cpu_id: usize, val: T) {
        let pre_cpu_map = unsafe { &mut *self.0.get() };
        pre_cpu_map.insert(cpu_id, Some(val));
    }

    pub fn get(&self, cpu_id: usize) -> Option<&T> {
        let pre_cpu_map = unsafe { &*self.0.get() };
        let v = pre_cpu_map.get(&cpu_id)?;
        Some(v.as_ref().expect("init not called"))
    }

    pub fn init(&self) {
        let cpu_list = Hal::cpu_list();
        debug!("Initializing PreCpuSet for CPUs: {:?}", cpu_list);
        for cpu_id in cpu_list {
            let v = unsafe { &mut *self.0.get() };
            v.insert(cpu_id.raw(), None);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CpuHardId(usize);

impl CpuHardId {
    pub fn new(id: usize) -> Self {
        CpuHardId(id)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}
