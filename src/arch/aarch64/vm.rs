use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity};

use super::AddrSpace;
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use arm_vcpu::Aarch64VCpuSetupConfig;

use crate::{
    GuestPhysAddr, HostPhysAddr, HostVirtAddr, RunError, TASK_STACK_SIZE, VmStatusInitOps,
    VmStatusRunningOps, VmStatusStoppingOps,
    arch::cpu::VCpu,
    config::{AxVMConfig, MemoryKind},
    fdt::fdt,
    region::GuestRegion,
    vhal::{ArchHal, cpu::CpuId, phys_to_virt, virt_to_phys},
    vm::{Status, VmId},
};

const VM_ASPACE_BASE: usize = 0x0;
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;

/// AArch64 Virtual Machine implementation
pub struct VmInit {
    pub id: VmId,
    pub name: String,
    pt_levels: usize,
    stop_requested: AtomicBool,
    exit_code: AtomicUsize,
    run_data: Option<VmStatusRunning>,
}

impl VmInit {
    /// Creates a new VM with the given configuration
    pub fn new(config: &AxVMConfig) -> anyhow::Result<Self> {
        let vm = Self {
            id: config.id().into(),
            name: config.name(),
            pt_levels: 4,
            stop_requested: AtomicBool::new(false),
            exit_code: AtomicUsize::new(0),
            run_data: None,
        };
        Ok(vm)
    }

    /// Initializes the VM, creating vCPUs and setting up memory
    pub fn init(&mut self, config: AxVMConfig) -> anyhow::Result<()> {
        debug!("Initializing VM {} ({})", self.id, self.name);

        // Create vCPUs
        let mut vcpus = Vec::new();

        let dtb_addr = GuestPhysAddr::from_usize(0);

        match config.cpu_num {
            crate::config::CpuNumType::Alloc(num) => {
                for i in 0..num {
                    let vcpu = VCpu::new(None, dtb_addr)?;
                    debug!("Created vCPU with {:?}", vcpu.id);
                    vcpus.push(vcpu);
                }
            }
            crate::config::CpuNumType::Fixed(ref ids) => {
                for id in ids {
                    let vcpu = VCpu::new(Some(*id), dtb_addr)?;
                    debug!("Created vCPU with {:?}", vcpu.id);
                    vcpus.push(vcpu);
                }
            }
        }

        let vcpu_count = vcpus.len();

        for vcpu in &vcpus {
            let max_levels = vcpu.with_hcpu(|cpu| cpu.max_guest_page_table_levels());
            if max_levels < self.pt_levels {
                self.pt_levels = max_levels;
            }
        }

        debug!(
            "VM {} ({}) vCPU count: {}, Max Guest Page Table Levels: {}",
            self.id, self.name, vcpu_count, self.pt_levels
        );

        // Create address space for the VM
        let address_space = AddrSpace::new_empty(
            self.pt_levels,
            axaddrspace::GuestPhysAddr::from(VM_ASPACE_BASE),
            VM_ASPACE_SIZE,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create address space: {:?}", e))?;

        let mut run_data = VmStatusRunning {
            vcpus,
            address_space,
            regions: Vec::new(),
            devices: BTreeMap::new(),
            kernel_entry: GuestPhysAddr::from_usize(0),
            dtb_addr: GuestPhysAddr::from_usize(0),
            dtb_data: Vec::new(),
            ramdisk_data: Vec::new(),
            bios_data: Vec::new(),
            vcpu_running_count: Arc::new(AtomicUsize::new(0)),
        };

        debug!("Mapping memory regions for VM {} ({})", self.id, self.name);
        for memory_cfg in &config.memory_regions {
            run_data.add_memory_region(memory_cfg)?;
        }

        debug!(
            "Mapped {} memory regions for VM {} ({})",
            run_data.regions.len(),
            self.id,
            self.name
        );

        run_data.load_images(&config)?;

        // Add emulated devices
        // for emu_device in config.emu_devices() {
        //     let device_info = DeviceInfo {
        //         device_type: DeviceType::Emulated,
        //         gpa: GuestPhysAddr::from(emu_device.base_gpa),
        //         hpa: None,
        //         size: emu_device.length,
        //         config: DeviceConfig::Mmio {
        //             flags: MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
        //         },
        //     };

        //     devices.insert(emu_device.name.clone(), device_info);

        //     // Map device memory
        //     self.map_region(
        //         GuestPhysAddr::from(emu_device.base_gpa),
        //         HostPhysAddr::from(emu_device.base_gpa), // Use identity mapping for emulated devices
        //         emu_device.length,
        //         MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
        //     )
        //     .map_err(|e| {
        //         anyhow::anyhow!("Failed to map emulated device {}: {:?}", emu_device.name, e)
        //     })?;
        // }

        // // Add passthrough devices
        // for pt_device in config.pass_through_devices() {
        //     let device_info = DeviceInfo {
        //         device_type: DeviceType::Passthrough,
        //         gpa: GuestPhysAddr::from(pt_device.base_gpa),
        //         hpa: Some(HostPhysAddr::from(pt_device.base_hpa)),
        //         size: pt_device.length,
        //         config: DeviceConfig::Mmio {
        //             flags: MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
        //         },
        //     };

        //     devices.insert(pt_device.name.clone(), device_info);

        //     // Map device memory
        //     self.map_region(
        //         GuestPhysAddr::from(pt_device.base_gpa),
        //         HostPhysAddr::from(pt_device.base_hpa),
        //         pt_device.length,
        //         MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
        //     )
        //     .map_err(|e| {
        //         anyhow::anyhow!(
        //             "Failed to map passthrough device {}: {:?}",
        //             pt_device.name,
        //             e
        //         )
        //     })?;
        // }

        // Setup vCPUs
        for vcpu in &mut run_data.vcpus {
            vcpu.vcpu.set_entry(run_data.kernel_entry).unwrap();
            vcpu.vcpu.set_dtb_addr(run_data.dtb_addr).unwrap();

            let setup_config = Aarch64VCpuSetupConfig {
                passthrough_interrupt: config.interrupt_mode()
                    == axvmconfig::VMInterruptMode::Passthrough,
                passthrough_timer: config.interrupt_mode()
                    == axvmconfig::VMInterruptMode::Passthrough,
            };

            vcpu.vcpu
                .setup(setup_config)
                .map_err(|e| anyhow::anyhow!("Failed to setup vCPU : {e:?}"))?;

            // Set EPT root
            vcpu.vcpu
                .set_ept_root(run_data.address_space.page_table_root())
                .map_err(|e| anyhow::anyhow!("Failed to set EPT root for vCPU : {e:?}"))?;

            run_data.vcpu_running_count.fetch_add(1, Ordering::SeqCst);
        }

        self.run_data = Some(run_data);

        Ok(())
    }
}

impl VmStatusInitOps for VmInit {
    type Running = VmStatusRunning;

    fn id(&self) -> VmId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn start(self) -> Result<Self::Running, (anyhow::Error, Self)> {
        let mut data = self.run_data.unwrap();

        let mut vcpus = vec![];

        vcpus.append(&mut data.vcpus);
        let mut vcpu_handles = vec![];
        let vm_id = self.id;

        for mut vcpu in vcpus.into_iter() {
            let vcpu_id = vcpu.id;
            let vcpu_running_count = data.vcpu_running_count.clone();
            let bind_id = vcpu.binded_cpu_id();
            let handle = std::thread::Builder::new()
                .name(format!("{vm_id}-{vcpu_id}"))
                .stack_size(TASK_STACK_SIZE)
                .spawn(move || {
                    assert!(
                        set_current_affinity(AxCpuMask::one_shot(bind_id.raw())),
                        "Initialize CPU affinity failed!"
                    );
                    match vcpu.run() {
                        Ok(()) => {
                            info!("vCPU {} of VM {} exited normally", vcpu_id, vm_id);
                        }
                        Err(e) => {
                            error!(
                                "vCPU {} of VM {} exited with error: {:?}",
                                vcpu_id, vm_id, e
                            );
                        }
                    }
                    vcpu_running_count.fetch_sub(1, Ordering::SeqCst);
                    vcpu
                })
                .unwrap();

            vcpu_handles.push(handle);
        }

        info!(
            "VM {} ({}) with {} cpus booted successfully.",
            self.id,
            self.name,
            vcpu_handles.len()
        );
        Ok(data)
    }
}

impl VmStatusRunningOps for VmStatusRunning {
    type Stopping = VmStatusStopping;

    fn stop(self) -> Result<Self::Stopping, (anyhow::Error, Self)>
    where
        Self: Sized,
    {
        Ok(VmStatusStopping {})
    }

    fn do_work(&mut self) -> Result<(), RunError> {
        if self.vcpu_running_count.load(Ordering::SeqCst) == 0 {
            Err(RunError::Exit)
        } else {
            Ok(())
        }
    }
}

pub struct VmStatusStopping {}

impl VmStatusStoppingOps for VmStatusStopping {}

/// Data needed when VM is running
pub struct VmStatusRunning {
    vcpus: Vec<VCpu>,
    address_space: AddrSpace,
    regions: Vec<GuestRegion>,
    devices: BTreeMap<String, DeviceInfo>,
    kernel_entry: GuestPhysAddr,
    dtb_addr: GuestPhysAddr,
    dtb_data: Vec<u32>,
    ramdisk_data: Vec<u8>,
    bios_data: Vec<u8>,
    vcpu_running_count: Arc<AtomicUsize>,
}

impl VmStatusRunning {
    fn add_memory_region(&mut self, config: &MemoryKind) -> anyhow::Result<()> {
        let region = GuestRegion::new(config);
        self.address_space
            .map_linear(
                region.gpa.as_usize().into(),
                region.hva.as_usize().into(),
                region.size,
                axaddrspace::MappingFlags::READ
                    | axaddrspace::MappingFlags::WRITE
                    | axaddrspace::MappingFlags::EXECUTE
                    | axaddrspace::MappingFlags::USER,
            )
            .map_err(|e| anyhow::anyhow!("Failed to map memory region: {:?}", e))?;

        self.regions.push(region);

        Ok(())
    }

    fn load_images(&mut self, config: &AxVMConfig) -> anyhow::Result<()> {
        // Load other images (BIOS, DTB, Ramdisk) similarly...
        debug!(
            "Loading kernel image for VM {} ({})",
            config.id(),
            config.name()
        );
        let _main_region_idx = self.load_kernel_image(config)?;
        self.load_dtb_image(config)?;

        Ok(())
    }

    /// Returns the loaded kernel region's index
    fn load_kernel_image(&mut self, config: &AxVMConfig) -> anyhow::Result<usize> {
        let mut idx = 0;
        let image_cfg = config.image_config();
        let gpa = if let Some(gpa) = image_cfg.kernel.gpa {
            let mut found = false;
            for (i, region) in self.regions.iter().enumerate() {
                if (region.gpa..region.gpa + region.size).contains(&gpa) {
                    idx = i;
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(anyhow!(
                    "Kernel load GPA {:#x} not within any memory region",
                    gpa.as_usize()
                ));
            }
            gpa
        } else {
            let mut gpa = None;
            for (i, region) in self.regions.iter().enumerate() {
                if region.size >= image_cfg.kernel.data.len() {
                    gpa = Some(region.gpa + 2 * 1024 * 1024);
                    idx = i;
                    break;
                } else {
                    continue;
                }
            }
            gpa.ok_or(anyhow!("No suitable memory region found for kernel image"))?
        };

        debug!(
            "Loading kernel image into GPA @{:#x} for VM {} ({})",
            gpa.as_usize(),
            config.id(),
            config.name()
        );
        self.load_image_data(gpa, &image_cfg.kernel.data)?;
        self.kernel_entry = gpa;

        Ok(idx)
    }

    fn load_dtb_image(&mut self, config: &AxVMConfig) -> anyhow::Result<()> {
        let image_cfg = config.image_config();

        if let Some(dtb_cfg) = &image_cfg.dtb {
            let size = dtb_cfg.data.len();
            self.dtb_data = Vec::with_capacity(size / 4);

            let gpa = if let Some(gpa) = dtb_cfg.gpa {
                gpa
            } else {
                (self.dtb_data.as_mut_ptr() as usize).into()
            };
            self.address_space
                .map_linear(
                    gpa.as_usize().into(),
                    virt_to_phys(HostVirtAddr::from(self.dtb_data.as_mut_ptr() as usize))
                        .as_usize()
                        .into(),
                    size,
                    axaddrspace::MappingFlags::READ | axaddrspace::MappingFlags::USER,
                )
                .map_err(|e| anyhow::anyhow!("Failed to map DTB region: {:?}", e))?;

            debug!(
                "Loading DTB image into GPA @{:#x} for VM {} ({})",
                gpa.as_usize(),
                config.id(),
                config.name()
            );
            self.dtb_addr = gpa;
            self.load_image_data(gpa, &dtb_cfg.data)?;
        } else {
            debug!(
                "No dtb provided, generating new dtb for {} ({})",
                config.id(),
                config.name()
            );
            let fdt = fdt().unwrap();
            let dtb_bytes = fdt.as_slice();
        }

        Ok(())
    }

    fn load_image_data(&mut self, gpa: GuestPhysAddr, data: &[u8]) -> anyhow::Result<()> {
        let hva = self
            .address_space
            .translated_byte_buffer(gpa.as_usize().into(), data.len())
            .ok_or(anyhow!("Fail to load [{gpa:?}, {:?})", gpa + data.len()))?;
        let mut remain = data;

        for buff in hva {
            let copy_size = core::cmp::min(remain.len(), buff.len());
            buff[..copy_size].copy_from_slice(&remain[..copy_size]);
            crate::arch::Hal::cache_flush(HostVirtAddr::from(buff.as_ptr() as usize), copy_size);
            remain = &remain[copy_size..];
            if remain.is_empty() {
                break;
            }
        }

        Ok(())
    }
}

/// Information about a device in the VM
#[derive(Debug, Clone)]
pub struct DeviceInfo {}
