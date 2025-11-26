use aarch64_cpu::registers::MPIDR_EL1;
use aarch64_cpu_ext::asm::cache;
use aarch64_cpu_ext::cache::{CacheOp, dcache_range};
use axhal::percpu::this_cpu_id;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use memory_addr::VirtAddr;

use crate::alloc::{collections::BTreeMap, string::String, vec::Vec};
use crate::arch::cpu::VCpu;
use crate::fdt;
use crate::vhal::{
    ArchHal,
    cpu::{CpuHardId, CpuId},
};

use aarch64_cpu::registers::{ReadWriteable, Readable, Writeable};
use axaddrspace::{AxMmHal, MappingFlags};
use axerrno::{AxResult, ax_err};
use page_table_multiarch::PagingHandler;

use crate::{config::AxVMConfig, vm::*};

pub mod cpu;
mod vm;

pub use cpu::HCpu;
pub use vm::*;

type AddrSpace = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;

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
        arm_vcpu::init_hal(&cpu::VCpuHal);
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

// Implement Display for VmId
impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VmId({:?})", self)
    }
}
