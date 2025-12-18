use alloc::vec::Vec;

use aarch64_cpu::registers::*;
use aarch64_cpu_ext::cache::{CacheOp, dcache_range};

use crate::fdt;
use crate::vhal::{
    ArchHal,
    cpu::{CpuHardId, CpuId},
};

use super::cpu::{HCpu, VCpuHal};

pub struct Hal;

impl ArchHal for Hal {
    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu> {
        info!("Enabling virtualization on cpu {id}");
        let mut cpu = HCpu::new(id);
        cpu.init()?;
        info!("{cpu}");
        Ok(cpu)
    }

    fn init() -> anyhow::Result<()> {
        arm_vcpu::init_hal(&VCpuHal);

        Ok(())
    }

    fn cpu_list() -> Vec<CpuHardId> {
        fdt::cpu_list()
            .unwrap()
            .into_iter()
            .map(CpuHardId::new)
            .collect()
    }

    fn cpu_hard_id() -> CpuHardId {
        let mpidr = MPIDR_EL1.get() as usize;
        CpuHardId::new(mpidr)
    }

    fn cache_flush(vaddr: arm_vcpu::HostVirtAddr, size: usize) {
        dcache_range(CacheOp::CleanAndInvalidate, vaddr.as_usize(), size);
    }
}
