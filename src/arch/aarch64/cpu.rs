use aarch64_cpu::registers::*;
use axhal::percpu::this_cpu_id;

use crate::{
    fdt,
    vhal::{ArchCpuData, ArchHal, CpuHardId, CpuId, precpu::PreCpuSet},
};

pub struct CpuData {
    pub id: CpuId,
    pub hard_id: CpuHardId,
}

impl CpuData {
    pub fn new(id: CpuId) -> Self {
        let mpidr = MPIDR_EL1.get() as usize;
        let hard_id = mpidr & 0xff_ff_ff;

        CpuData {
            id,
            hard_id: CpuHardId::new(hard_id),
        }
    }
}

impl ArchCpuData for CpuData {
    fn hard_id(&self) -> crate::vhal::CpuHardId {
        self.hard_id
    }
}
