use alloc::{collections::BTreeMap, vec::Vec};
use bitmap_allocator::{BitAlloc, BitAlloc4K};
use core::{
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};
use spin::Mutex;

use crate::{
    arch::{HCpu, Hal},
    vhal::{
        cpu::{CpuHardId, CpuId},
        precpu::PreCpuSet,
    },
};
use axconfig::TASK_STACK_SIZE;
use axtask::AxCpuMask;

pub(crate) mod cpu;
pub(crate) mod precpu;
mod timer;

pub fn init() -> anyhow::Result<()> {
    Hal::init()?;

    static CORES: AtomicUsize = AtomicUsize::new(0);

    let cpu_count = cpu_count();

    info!("Initializing VHal for {cpu_count} CPUs...");
    cpu::PRE_CPU.init();
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
                unsafe { cpu::PRE_CPU.set(cpu_data.hard_id(), cpu_data) };
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

    cpu::HCPU_ALLOC.lock().insert(0..cpu_count);

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
    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu>;
}

pub(crate) trait ArchCpuData {
    fn hard_id(&self) -> CpuHardId;
}
