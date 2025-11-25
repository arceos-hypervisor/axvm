use core::fmt::Display;

use aarch64_cpu::registers::*;
use alloc::sync::Weak;
use arm_vcpu::{Aarch64PerCpu, Aarch64VCpuCreateConfig};
use axhal::percpu::this_cpu_id;
use axvm_types::addr::*;

use crate::vhal::{
    ArchCpuData,
    cpu::{CpuHardId, CpuId, HCpuExclusive},
};

pub struct HCpu {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    vpercpu: Aarch64PerCpu,
    max_guest_page_table_levels: usize,
}

impl HCpu {
    pub fn new(id: CpuId) -> Self {
        let mpidr = MPIDR_EL1.get() as usize;
        let hard_id = mpidr & 0xff_ff_ff;

        let vpercpu = Aarch64PerCpu::new();

        HCpu {
            id,
            hard_id: CpuHardId::new(hard_id),
            vpercpu,
            max_guest_page_table_levels: 0,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        self.vpercpu.hardware_enable();
        self.max_guest_page_table_levels = self.vpercpu.max_guest_page_table_levels();
        Ok(())
    }

    pub fn max_guest_page_table_levels(&self) -> usize {
        self.max_guest_page_table_levels
    }
}

impl ArchCpuData for HCpu {
    fn hard_id(&self) -> CpuHardId {
        self.hard_id
    }
}

impl Display for HCpu {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "
CPU {}:
  Hard ID: {}
  PT Levels: {}",
            self.id, self.hard_id, self.max_guest_page_table_levels
        )
    }
}

pub(super) struct VCpuHal;

impl arm_vcpu::CpuHal for VCpuHal {
    fn irq_hanlder(&self) {
        axhal::irq::irq_handler(0);
    }

    fn inject_interrupt(&self, irq: usize) {
        todo!()
    }
}

pub struct VCpu {
    pub id: CpuHardId,
    pub vcpu: arm_vcpu::Aarch64VCpu,
    hcpu: HCpuExclusive,
}

impl VCpu {
    pub fn new(host_cpuid: Option<CpuId>, dtb_addr: GuestPhysAddr) -> anyhow::Result<Self> {
        let hcpu_exclusive = HCpuExclusive::try_new(host_cpuid)
            .ok_or_else(|| anyhow!("Failed to allocate cpu with id `{host_cpuid:?}`"))?;

        let hard_id = hcpu_exclusive.hard_id();

        let vcpu = arm_vcpu::Aarch64VCpu::new(Aarch64VCpuCreateConfig {
            mpidr_el1: hard_id.raw() as u64,
            dtb_addr: dtb_addr.as_usize(),
        })
        .unwrap();
        Ok(VCpu {
            id: hard_id,
            vcpu,
            hcpu: hcpu_exclusive,
        })
    }

    pub fn with_hcpu<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HCpu) -> R,
    {
        self.hcpu.with_cpu(f)
    }
}
