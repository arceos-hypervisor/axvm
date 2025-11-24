use core::fmt::Display;

use aarch64_cpu::registers::*;
use alloc::sync::Weak;
use arm_vcpu::Aarch64PerCpu;
use axhal::percpu::this_cpu_id;

use crate::{
    fdt,
    vhal::{ArchCpuData, ArchHal, CpuHardId, CpuId, precpu::PreCpuSet},
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
    fn hard_id(&self) -> crate::vhal::CpuHardId {
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
    pub v_hard_id: CpuHardId,
    pub vcpu: arm_vcpu::Aarch64VCpu,
    hcpu: CpuHardId,
}

impl VCpu {
    // pub fn new(config: &) -> Self {
    //     let vcpu = arm_vcpu::Aarch64VCpu::new(VCpuHal);

    //     VCpu {
    //         v_hard_id,
    //         vcpu,
    //         hcpu: hcpu_id,
    //     }
    // }
}