use alloc::vec::Vec;

use crate::hal::{ArchOp, cpu::{CpuHardId, CpuId}};
use crate::fdt;

use super::cpu::{HCpu, VCpuHal};

pub struct Hal;

impl ArchOp for Hal {
    type HCPU = HCpu;

    fn init() -> anyhow::Result<()> {
        riscv_vcpu::init_hal(&VCpuHal);
        Ok(())
    }

    fn cache_flush(_vaddr: riscv_vcpu::HostVirtAddr, _size: usize) {
        // TODO: Implement cache flush for RISC-V
        // RISC-V has cache management instructions but they are optional
    }

    fn cpu_list() -> Vec<CpuHardId> {
        fdt::cpu_list()
            .unwrap()
            .into_iter()
            .map(CpuHardId::new)
            .collect()
    }

    fn cpu_hard_id() -> CpuHardId {
        CpuHardId::new(axplat::percpu::this_cpu_id())
    }

    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu> {
        info!("Enabling virtualization on riscv cpu {id}");
        let mut cpu = HCpu::new(id);
        cpu.init()?;
        info!("{cpu}");
        Ok(cpu)
    }
}
