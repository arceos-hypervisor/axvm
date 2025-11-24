use aarch64_cpu::registers::MPIDR_EL1;
use axhal::percpu::this_cpu_id;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use memory_addr::VirtAddr;

use crate::alloc::alloc::{self, Layout};
use crate::alloc::collections::BTreeMap;
use crate::alloc::string::String;
use crate::alloc::sync::Arc;
use crate::alloc::vec;
use crate::alloc::vec::Vec;
use crate::fdt;
use crate::vhal::{ArchHal, CpuId};

use aarch64_cpu::registers::{ReadWriteable, Readable, Writeable};
use axaddrspace::{AddrSpace, AxMmHal, GuestPhysAddr, HostPhysAddr, MappingFlags};
use axerrno::{AxResult, ax_err};
use page_table_multiarch::PagingHandler;

use crate::{config::AxVMConfig, vm::*};

pub mod cpu;
mod vm;

pub use cpu::HCpu;
pub use vm::*;

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

    fn cpu_list() -> Vec<crate::vhal::CpuHardId> {
        fdt::cpu_list()
            .unwrap()
            .into_iter()
            .map(crate::vhal::CpuHardId::new)
            .collect()
    }

    fn cpu_hard_id() -> crate::vhal::CpuHardId {
        let mpidr = MPIDR_EL1.get() as usize;
        crate::vhal::CpuHardId::new(mpidr)
    }
}

// Implement Display for VmId
impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VmId({:?})", self)
    }
}

/// Data needed when VM is running
pub struct RunData {
    // vcpus: BTreeMap<usize, AxVCpuRef<DummyHal>>,
    // address_space: AddrSpace<DummyPagingHandler>,
    devices: BTreeMap<String, DeviceInfo>,
}

/// Information about a device in the VM
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device type (emulated or passthrough)
    pub device_type: DeviceType,
    /// Base address in guest physical memory
    pub gpa: GuestPhysAddr,
    /// Base address in host physical memory (for passthrough)
    pub hpa: Option<HostPhysAddr>,
    /// Size of the device memory region
    pub size: usize,
    /// Device-specific configuration
    pub config: DeviceConfig,
}

/// Device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Emulated device
    Emulated,
    /// Passthrough device
    Passthrough,
}

/// Device-specific configuration
#[derive(Debug, Clone)]
pub enum DeviceConfig {
    /// Generic MMIO device
    Mmio {
        /// Access flags
        flags: MappingFlags,
    },
    /// Generic PCI device
    Pci {
        /// PCI bus number
        bus: u8,
        /// PCI device number
        device: u8,
        /// PCI function number
        function: u8,
    },
    /// Interrupt controller
    InterruptController {
        /// Controller type (GICv2, GICv3, etc.)
        controller_type: String,
        /// Number of interrupt lines
        num_interrupts: u32,
    },
    /// Timer device
    Timer {
        /// Timer type
        timer_type: String,
    },
    /// Other device type
    Other {
        /// Device-specific data
        data: Vec<u8>,
    },
}
