use alloc::string::String;
use alloc::vec::Vec;

use axaddrspace::GuestPhysAddr;
use axdevice_base::EmulatedDeviceConfig;
use axerrno::AxResult;

/// A part of `AxVCpuConfig`, which represents an architecture-dependent `VCpu`.
///
/// The concrete type of configuration is defined in `AxArchVCpuImpl`.
// #[derive(Clone, Copy, Debug, Default)]
// pub struct AxArchVCpuConfig<H: AxVMHal> {
//     pub create_config: <AxArchVCpuImpl<H> as AxArchVCpu>::CreateConfig,
//     pub setup_config: <AxArchVCpuImpl<H> as AxArchVCpu>::SetupConfig,
// }

/// A part of `AxVMConfig`, which represents a `VCpu`.
#[derive(Clone, Copy, Debug, Default)]
pub struct AxVCpuConfig {
    // pub arch_config: AxArchVCpuConfig,
    pub bsp_entry: GuestPhysAddr,
    pub ap_entry: GuestPhysAddr,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum VMType {
    VMTHostVM = 0,
    #[default]
    VMTRTOS = 1,
    VMTLinux = 2,
}

impl From<usize> for VMType {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::VMTHostVM,
            1 => Self::VMTRTOS,
            2 => Self::VMTLinux,
            _ => {
                warn!("Unknown VmType value: {}, default to VMTRTOS", value);
                Self::default()
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct VMImageConfig {
    // pub kernel_img_size: usize,
    pub kernel_load_gpa: GuestPhysAddr,

    // pub bios_img_size: Option<usize>,
    pub bios_load_gpa: Option<GuestPhysAddr>,
    // pub dtb_img_size: Option<usize>,
    pub dtb_load_gpa: Option<GuestPhysAddr>,
    // pub ramdisk_img_size: Option<usize>,
    pub ramdisk_load_gpa: Option<GuestPhysAddr>,
}

/// A part of `AxVMCrateConfig`, which represents a `VM`.
#[derive(Debug, Default)]
pub struct AxVMConfig {
    id: usize,
    name: String,
    #[allow(dead_code)]
    vm_type: VMType,

    cpu_num: usize,
    phys_cpu_sets: Option<Vec<usize>>,
    cpu_config: AxVCpuConfig,

    image_config: VMImageConfig,

    memory_regions: Vec<VmMemConfig>,

    emu_devices: Vec<EmulatedDeviceConfig>,
    // To be added: device configuration
}

impl From<AxVMCrateConfig> for AxVMConfig {
    fn from(cfg: AxVMCrateConfig) -> Self {
        Self {
            id: cfg.id,
            name: cfg.name,
            vm_type: VMType::from(cfg.vm_type),
            cpu_num: cfg.cpu_num,
            phys_cpu_sets: cfg.phys_cpu_sets,
            cpu_config: AxVCpuConfig {
                bsp_entry: GuestPhysAddr::from(cfg.entry_point),
                ap_entry: GuestPhysAddr::from(cfg.entry_point),
            },
            image_config: VMImageConfig {
                kernel_load_gpa: GuestPhysAddr::from(cfg.kernel_load_addr),
                bios_load_gpa: cfg.bios_load_addr.map(|addr| GuestPhysAddr::from(addr)),
                dtb_load_gpa: cfg.dtb_load_addr.map(|addr| GuestPhysAddr::from(addr)),
                ramdisk_load_gpa: cfg.ramdisk_load_addr.map(|addr| GuestPhysAddr::from(addr)),
            },
            memory_regions: cfg.memory_regions,
            emu_devices: cfg.emu_devices,
        }
    }
}

impl AxVMConfig {
    /// Returns VM id.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns VM name.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Returns vCpu id list and its corresponding pCpu affinity list.
    /// If the pCpu affinity is None, it means the vCpu will be allocated to any available pCpu randomly.
    pub fn get_vcpu_affinities(&self) -> Vec<(usize, Option<usize>)> {
        let mut vcpu_pcpu_pairs = Vec::new();
        for vcpu_id in 0..self.cpu_num {
            vcpu_pcpu_pairs.push((vcpu_id, None));
        }
        if let Some(phys_cpu_sets) = &self.phys_cpu_sets {
            for (vcpu_id, pcpu_mask_bitmap) in phys_cpu_sets.iter().enumerate() {
                vcpu_pcpu_pairs[vcpu_id].1 = Some(*pcpu_mask_bitmap);
            }
        }
        vcpu_pcpu_pairs
    }

    pub fn image_config(&self) -> &VMImageConfig {
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

    pub fn memory_regions(&self) -> &Vec<VmMemConfig> {
        &self.memory_regions
    }

    pub fn emu_devices(&self) -> &Vec<EmulatedDeviceConfig> {
        &self.emu_devices
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct VmMemConfig {
    pub gpa: usize,
    pub size: usize,
    pub flags: usize,
}

/// The configuration of axvm crate. It's not used yet, may be used in the future.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct AxVMCrateConfig {
    // Basic Information
    id: usize,
    name: String,
    vm_type: usize,

    // Resources.
    // The number of virtual CPUs.
    cpu_num: usize,
    // The mask of physical CPUs who can run this VM.
    // * If None, vcpu will be scheduled on available physical CPUs randomly.
    // * If set, each vcpu will be scheduled on the specified physical CPUs.
    //      For example, [0x0101, 0x0010] means:
    //          * vCpu0 can be scheduled at pCpu0 and pCpu2;
    //          * vCpu1 will only be scheduled at pCpu1;
    //      It will phrase an error if the number of vCpus is not equal to the length of `phys_cpu_sets` array.
    phys_cpu_sets: Option<Vec<usize>>,

    entry_point: usize,

    // VM image infos.
    pub kernel_path: String,
    pub kernel_load_addr: usize,
    pub bios_path: Option<String>,
    pub bios_load_addr: Option<usize>,
    pub dtb_path: Option<String>,
    pub dtb_load_addr: Option<usize>,
    pub ramdisk_path: Option<String>,
    pub ramdisk_load_addr: Option<usize>,

    disk_path: Option<String>,

    /// Memory Information
    memory_regions: Vec<VmMemConfig>,
    // Todo:
    // Device Information
    /// Emu device Information
    emu_devices: Vec<EmulatedDeviceConfig>,
}

impl AxVMCrateConfig {
    pub fn from_toml(raw_cfg_str: &str) -> AxResult<Self> {
        let config: AxVMCrateConfig = toml::from_str(raw_cfg_str).map_err(|err| {
            axerrno::ax_err_type!(
                InvalidInput,
                alloc::format!("toml deserialize get err {err:?}")
            )
        })?;
        Ok(config)
    }
}
