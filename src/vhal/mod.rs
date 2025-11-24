use alloc::{collections::BTreeMap, vec::Vec};
use core::{
    cell::UnsafeCell,
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};
use spin::Mutex;
use vm_allocator::IdAllocator;

use crate::{
    arch::{HCpu, Hal},
    vhal::precpu::PreCpuSet,
};
use axconfig::TASK_STACK_SIZE;
use axtask::AxCpuMask;

pub(crate) mod precpu;
mod timer;

static PRE_CPU: PreCpuSet<HCpu> = PreCpuSet::new();
static HCPU_ALLOC: Mutex<Option<IdAllocator>> = Mutex::new(None);

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
        // handles.push(_handle);
    }
    info!("Waiting for all cores to enable hardware virtualization...");

    // Wait for all cores to enable virtualization.
    while CORES.load(Ordering::Acquire) != cpu_count {
        // Use `yield_now` instead of `core::hint::spin_loop` to avoid deadlock.
        axtask::yield_now();
    }
    // for handle in handles {
    //     handle.join();
    // }

    HCPU_ALLOC.lock().replace(IdAllocator::new(0, cpu_count));

    info!("All cores have enabled hardware virtualization support.");
    Ok(())
}

pub fn cpu_count() -> usize {
    axruntime::cpu_count()
}

pub struct HCpuExclusive(CpuId);

impl HCpuExclusive {
    pub fn try_new(id: Option<CpuId>) -> Option<Self> {
        // let id = id.unwrap_or_else(|| {
        //     let hard_id = Hal::cpu_hard_id();
        //     let cpu_list = Hal::cpu_list();
        //     let index = cpu_list
        //         .iter()
        //         .position(|&h_id| h_id == hard_id)
        //         .expect("Current CPU hard ID not found in CPU list");
        //     CpuId::new(index)
        // });
        // let cpu_data = PRE_CPU.get(id).ok()?;
        // Some(HCpuExclusive(id))
    }
}

impl Drop for HCpuExclusive {
    fn drop(&mut self) {
        let mut allocator = HCPU_ALLOC.lock();
        if let Some(ref mut alloc) = *allocator {
            let _ = alloc.free_id(self.0.raw() as u32);
        }
    }
}

pub(crate) trait ArchHal {
    fn init() -> anyhow::Result<()>;
    fn cpu_hard_id() -> CpuHardId;
    fn cpu_list() -> Vec<CpuHardId>;
    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu>;
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
