use alloc::{collections::BTreeMap, vec::Vec};
use core::{
    cell::UnsafeCell,
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};

use axtask::AxCpuMask;

use crate::{
    TASK_STACK_SIZE,
    arch::{CpuData, Hal},
    vhal::precpu::PreCpuSet,
};

pub(crate) mod precpu;
mod timer;

static PRE_CPU: PreCpuSet<CpuData> = PreCpuSet::new();

pub fn init() -> anyhow::Result<()> {
    Hal::init()?;

    static CORES: AtomicUsize = AtomicUsize::new(0);

    let cpu_count = cpu_count();

    info!("Initializing VHal for {cpu_count} CPUs...");
    PRE_CPU.init();

    for cpu_id in 0..cpu_count {
        let id = CpuId::new(cpu_id);
        let _handle = axtask::spawn_raw(
            move || {
                info!("Core {cpu_id} is initializing hardware virtualization support...");
                // Initialize cpu affinity here.
                assert!(
                    axtask::set_current_affinity(AxCpuMask::one_shot(cpu_id)),
                    "Initialize CPU affinity failed!"
                );
                info!("Enabling hardware virtualization support on core {id}");
                timer::init_percpu();

                let cpu_data = Hal::current_cpu_init(id).expect("Enable virtualization failed!");
                unsafe { PRE_CPU.set(cpu_data.hard_id(), cpu_data) };
                let _ = CORES.fetch_add(1, Ordering::Release);
            },
            format!("init-cpu-{}", cpu_id),
            TASK_STACK_SIZE,
        );
        // _handle.join();
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
    fn cpu_hard_id() -> CpuHardId;
    fn cpu_list() -> Vec<CpuHardId>;
    fn current_cpu_init(id: CpuId) -> anyhow::Result<CpuData>;
}

pub(crate) trait ArchCpuData {
    fn hard_id(&self) -> CpuHardId;
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

impl Display for CpuHardId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CPU Hard({:#x})", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CpuId(usize);

impl CpuId {
    pub fn new(id: usize) -> Self {
        CpuId(id)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}

impl Display for CpuId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CPU({})", self.0)
    }
}
