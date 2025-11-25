//! The configuration structure for the VM.
//! The `AxVMCrateConfig` is generated from toml file, and then converted to `AxVMConfig` for the VM creation.

use alloc::string::String;
use alloc::vec::Vec;

use axvm_types::addr::GuestPhysAddr;

pub use axvmconfig::{
    AxVMCrateConfig, EmulatedDeviceConfig, PassThroughAddressConfig, PassThroughDeviceConfig,
    VMInterruptMode, VMType, VmMemConfig, VmMemMappingType,
};

use crate::vhal::cpu::CpuId;

// /// A part of `AxVCpuConfig`, which represents an architecture-dependent `VCpu`.
// ///
// /// The concrete type of configuration is defined in `AxArchVCpuImpl`.
// #[derive(Clone, Copy, Debug, Default)]
// pub struct AxArchVCpuConfig<H: AxVMHal> {
//     pub create_config: <AxArchVCpuImpl<H> as AxArchVCpu>::CreateConfig,
//     pub setup_config: <AxArchVCpuImpl<H> as AxArchVCpu>::SetupConfig,
// }

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

/// A part of `AxVMCrateConfig`, which represents a `VM`.
#[derive(Debug, Default)]
pub struct AxVMConfig {
    pub id: usize,
    pub name: String,
    pub cpu_num: CpuNumType,
    pub cpu_config: AxVCpuConfig,
    pub image_config: VMImagesConfig,
    pub emu_devices: Vec<EmulatedDeviceConfig>,
    pub pass_through_devices: Vec<PassThroughDeviceConfig>,
    pub excluded_devices: Vec<Vec<String>>,
    pub pass_through_addresses: Vec<PassThroughAddressConfig>,
    // TODO: improve interrupt passthrough
    pub spi_list: Vec<u32>,
    pub interrupt_mode: VMInterruptMode,
}

#[derive(Debug, Clone)]
pub enum CpuNumType {
    Alloc(usize),
    Fixed(Vec<CpuId>),
}

impl Default for CpuNumType {
    fn default() -> Self {
        CpuNumType::Alloc(1)
    }
}

// impl From<AxVMCrateConfig> for AxVMConfig {
//     fn from(cfg: AxVMCrateConfig) -> Self {
//         Self {
//             id: cfg.base.id,
//             name: cfg.base.name,
//             vm_type: VMType::from(cfg.base.vm_type),
//             phys_cpu_ls: PhysCpuList {
//                 cpu_num: cfg.base.cpu_num,
//                 phys_cpu_ids: cfg.base.phys_cpu_ids,
//                 phys_cpu_sets: cfg.base.phys_cpu_sets,
//             },
//             cpu_config: AxVCpuConfig {
//                 bsp_entry: GuestPhysAddr::from(cfg.kernel.entry_point),
//                 ap_entry: GuestPhysAddr::from(cfg.kernel.entry_point),
//             },
//             image_config: VMImagesConfig {
//                 kernel_load_gpa: GuestPhysAddr::from(cfg.kernel.kernel_load_addr),
//                 bios_load_gpa: cfg.kernel.bios_load_addr.map(GuestPhysAddr::from),
//                 dtb_load_gpa: cfg.kernel.dtb_load_addr.map(GuestPhysAddr::from),
//                 ramdisk_load_gpa: cfg.kernel.ramdisk_load_addr.map(GuestPhysAddr::from),
//             },
//             // memory_regions: cfg.kernel.memory_regions,
//             emu_devices: cfg.devices.emu_devices,
//             pass_through_devices: cfg.devices.passthrough_devices,
//             excluded_devices: cfg.devices.excluded_devices,
//             pass_through_addresses: cfg.devices.passthrough_addresses,
//             spi_list: Vec::new(),
//             interrupt_mode: cfg.devices.interrupt_mode,
//         }
//     }
// }

impl AxVMConfig {
    /// Returns VM id.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns VM name.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Returns configurations related to VM image load addresses.
    pub fn image_config(&self) -> &VMImagesConfig {
        &self.image_config
    }

    /// Returns the entry address in GPA for the Bootstrap Processor (BSP).
    pub fn bsp_entry(&self) -> GuestPhysAddr {
        // Retrieves BSP entry from the CPU configuration.
        self.cpu_config.bsp_entry
    }

    /// Returns the entry address in GPA for the Application Processor (AP).
    pub fn ap_entry(&self) -> GuestPhysAddr {
        // Retrieves AP entry from the CPU configuration.
        self.cpu_config.ap_entry
    }

    pub fn excluded_devices(&self) -> &Vec<Vec<String>> {
        &self.excluded_devices
    }

    pub fn pass_through_addresses(&self) -> &Vec<PassThroughAddressConfig> {
        &self.pass_through_addresses
    }
    // /// Returns configurations related to VM memory regions.
    // pub fn memory_regions(&self) -> Vec<VmMemConfig> {
    //     &self.memory_regions
    // }

    // /// Adds a new memory region to the VM configuration.
    // pub fn add_memory_region(&mut self, region: VmMemConfig) {
    //     self.memory_regions.push(region);
    // }

    // /// Checks if the VM memory regions contain a specific range.
    // pub fn contains_memory_range(&self, range: &Range<usize>) -> bool {
    //     self.memory_regions
    //         .iter()
    //         .any(|region| region.gpa <= range.start && region.gpa + region.size >= range.end)
    // }

    /// Returns configurations related to VM emulated devices.
    pub fn emu_devices(&self) -> &Vec<EmulatedDeviceConfig> {
        &self.emu_devices
    }

    /// Returns configurations related to VM passthrough devices.
    pub fn pass_through_devices(&self) -> &Vec<PassThroughDeviceConfig> {
        &self.pass_through_devices
    }

    /// Adds a new passthrough device to the VM configuration.
    pub fn add_pass_through_device(&mut self, device: PassThroughDeviceConfig) {
        self.pass_through_devices.push(device);
    }

    /// Removes passthrough device from the VM configuration.
    pub fn remove_pass_through_device(&mut self, device: PassThroughDeviceConfig) {
        self.pass_through_devices.retain(|d| d == &device);
    }

    /// Clears all passthrough devices from the VM configuration.
    pub fn clear_pass_through_devices(&mut self) {
        self.pass_through_devices.clear();
    }

    /// Adds a passthrough SPI to the VM configuration.
    pub fn add_pass_through_spi(&mut self, spi: u32) {
        self.spi_list.push(spi);
    }

    /// Returns the list of passthrough SPIs.
    pub fn pass_through_spis(&self) -> &Vec<u32> {
        &self.spi_list
    }

    /// Returns the interrupt mode of the VM.
    pub fn interrupt_mode(&self) -> VMInterruptMode {
        self.interrupt_mode
    }
}
