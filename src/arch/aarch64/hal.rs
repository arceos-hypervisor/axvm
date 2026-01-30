use alloc::vec::Vec;

use aarch64_cpu::registers::*;
use aarch64_cpu_ext::cache::{CacheOp, dcache_range};

use super::cpu::{HCpu, VCpuHal};
use crate::arch::cpu::CPUState;
use crate::hal::cpu::{CpuHardId, CpuId};
use crate::{VmWeak, fdt};

pub struct Hal;

impl crate::hal::ArchOp for Hal {
    type HCPU = HCpu;
    type VCPU = CPUState;

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

    fn new_vcpu(hard_id: CpuHardId, vm: VmWeak) -> anyhow::Result<Self::VCPU> {
        let vcpu = CPUState::new(hard_id, vm)?;
        Ok(vcpu)
    }
}
