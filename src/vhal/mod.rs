use alloc::{collections::BTreeMap, vec::Vec};
use axstd::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    thread::yield_now,
};
use bitmap_allocator::{BitAlloc, BitAlloc4K};
use core::{
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};
use spin::Mutex;

use crate::{
    HostPhysAddr, HostVirtAddr, TASK_STACK_SIZE,
    arch::{HCpu, Hal},
    vhal::{
        cpu::{CpuHardId, CpuId},
        precpu::PreCpuSet,
    },
};

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
        axstd::thread::Builder::new()
            .name(format!("init-cpu-{}", cpu_id))
            .stack_size(TASK_STACK_SIZE)
            .spawn(move || {
                info!("Core {cpu_id} is initializing hardware virtualization support...");
                // Initialize cpu affinity here.
                assert!(
                    set_current_affinity(AxCpuMask::one_shot(cpu_id)),
                    "Initialize CPU affinity failed!"
                );
                info!("Enabling hardware virtualization support on core {id}");
                timer::init_percpu();

                // let cpu_data = Hal::current_cpu_init(id).expect("Enable virtualization failed!");
                // unsafe { cpu::PRE_CPU.set(cpu_data.hard_id(), cpu_data) };
                let _ = CORES.fetch_add(1, Ordering::Release);
            })
            .map_err(|e| anyhow!("{e:?}"))?;
    }
    info!("Waiting for all cores to enable hardware virtualization...");

    // Wait for all cores to enable virtualization.
    while CORES.load(Ordering::Acquire) != cpu_count {
        // Use `yield_now` instead of `core::hint::spin_loop` to avoid deadlock.
        yield_now();
    }

    cpu::HCPU_ALLOC.lock().insert(0..cpu_count);

    info!("All cores have enabled hardware virtualization support.");



    Ok(())
}

pub fn cpu_count() -> usize {
    axruntime::cpu_count()
}

pub(crate) trait ArchHal {
    fn init() -> anyhow::Result<()>;
    fn cache_flush(vaddr: HostVirtAddr, size: usize);
    fn cpu_hard_id() -> CpuHardId;
    fn cpu_list() -> Vec<CpuHardId>;
    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu>;
}

pub(crate) trait ArchCpuData {
    fn hard_id(&self) -> CpuHardId;
}

pub fn phys_to_virt(paddr: HostPhysAddr) -> HostVirtAddr {
    axhal::mem::phys_to_virt(paddr.as_usize().into())
        .as_usize()
        .into()
}

pub fn virt_to_phys(vaddr: HostVirtAddr) -> HostPhysAddr {
    axhal::mem::virt_to_phys(vaddr.as_usize().into())
        .as_usize()
        .into()
}
