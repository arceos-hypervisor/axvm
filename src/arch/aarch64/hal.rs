use alloc::vec::Vec;

use aarch64_cpu::registers::*;
use aarch64_cpu_ext::cache::{CacheOp, dcache_range};

use super::cpu::{HCpu, VCpuHal};
use crate::fdt;
use crate::hal::cpu::{CpuHardId, CpuId};

pub struct Hal;

impl crate::hal::ArchOp for Hal {
    type HCPU = HCpu;

    fn init() -> anyhow::Result<()> {
        arm_vcpu::init_hal(&VCpuHal);
        Ok(())
    }

    fn cache_flush(vaddr: arm_vcpu::HostVirtAddr, size: usize) {
        dcache_range(CacheOp::CleanAndInvalidate, vaddr.as_usize(), size);
    }

    fn cpu_hard_id() -> CpuHardId {
        let mpidr = MPIDR_EL1.get() as usize & 0xffffff;
        CpuHardId::new(mpidr)
    }

    fn cpu_list() -> Vec<CpuHardId> {
        fdt::cpu_list()
            .unwrap()
            .into_iter()
            .map(CpuHardId::from)
            .collect()
    }

    fn current_cpu_init(id: crate::hal::cpu::CpuId) -> anyhow::Result<Self::HCPU> {
        info!("Enabling virtualization on cpu {id}");
        let mut cpu = HCpu::new(id);
        cpu.init()?;
        info!("{cpu}");
        Ok(cpu)
    }
}
