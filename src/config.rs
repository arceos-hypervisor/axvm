//! The configuration structure for the VM.
//! The `AxVMCrateConfig` is generated from toml file, and then converted to `AxVMConfig` for the VM creation.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{GuestPhysAddr, HostPhysAddr};

pub use axvmconfig::{
    AxVMCrateConfig, EmulatedDeviceConfig, PassThroughAddressConfig, PassThroughDeviceConfig,
    VMInterruptMode, VMType, VmMemConfig, VmMemMappingType,
};

use crate::vhal::cpu::CpuId;

/// A part of `AxVMConfig`, which represents a `VCpu`.
#[derive(Clone, Copy, Debug, Default)]
pub struct AxVCpuConfig {
    // pub arch_config: AxArchVCpuConfig,
    /// The entry address in GPA for the Bootstrap Processor (BSP).
    pub bsp_entry: GuestPhysAddr,
    /// The entry address in GPA for the Application Processor (AP).
    pub ap_entry: GuestPhysAddr,
}

#[derive(Debug, Default, Clone)]
pub struct VMImageConfig {
    pub gpa: Option<GuestPhysAddr>,
    pub data: Vec<u8>,
}

/// A part of `AxVMConfig`, which stores configuration attributes related to the load address of VM images.
#[derive(Debug, Default, Clone)]
pub struct VMImagesConfig {
    /// The load address in GPA for the kernel image.
    pub kernel: VMImageConfig,
    /// The load address in GPA for the BIOS image, `None` if not used.
    pub bios: Option<VMImageConfig>,
    /// The load address in GPA for the device tree blob (DTB), `None` if not used.
    pub dtb: Option<VMImageConfig>,
    /// The load address in GPA for the ramdisk image, `None` if not used.
    pub ramdisk: Option<VMImageConfig>,
}

#[derive(Debug, Clone)]
pub enum MemoryKind {
    /// Use identical memory regions
    Identical { size: usize },
    /// Use memory regions mapped from host physical address
    Passthrough { hpa: HostPhysAddr, size: usize },
    /// Use fixed memory regions
    Vmem { gpa: GuestPhysAddr, size: usize },
}

/// A part of `AxVMCrateConfig`, which represents a `VM`.
#[derive(Debug, Default)]
pub struct AxVMConfig {
    pub id: usize,
    pub name: String,
    pub cpu_num: CpuNumType,
    pub image_config: VMImagesConfig,
    pub memory_regions: Vec<MemoryKind>,
    pub interrupt_mode: VMInterruptMode,
}

#[derive(Debug, Clone)]
pub enum CpuNumType {
    Alloc(usize),
    Fixed(Vec<CpuId>),
}

impl CpuNumType {
    pub fn num(&self) -> usize {
        match self {
            CpuNumType::Alloc(num) => *num,
            CpuNumType::Fixed(ids) => ids.len(),
        }
    }
}

impl Default for CpuNumType {
    fn default() -> Self {
        CpuNumType::Alloc(1)
    }
}

impl AxVMConfig {
    /// Returns VM id.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns VM name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns configurations related to VM image load addresses.
    pub fn image_config(&self) -> &VMImagesConfig {
        &self.image_config
    }

    /// Returns the interrupt mode of the VM.
    pub fn interrupt_mode(&self) -> VMInterruptMode {
        self.interrupt_mode
    }
}
