use core::ops::Deref;

use alloc::vec::Vec;

use crate::{
    AxVMConfig, GuestPhysAddr, VmAddrSpace, VmMachineUninitOps,
    arch::{VmMachineInited, cpu::VCpu},
    config::CpuNumType,
    data::VmDataWeak,
    fdt::FdtBuilder,
};

#[cfg(feature = "virtio-blk")]
use axvmconfig::EmulatedDeviceType;

#[cfg(feature = "virtio-console")]
use axvmconfig::EmulatedDeviceType as EmulatedDeviceTypeConsole;

pub struct VmMachineUninit {
    config: AxVMConfig,
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
}

impl VmMachineUninitOps for VmMachineUninit {
    type Inited = VmMachineInited;

    fn new(config: AxVMConfig) -> Self {
        Self {
            config,
            pt_levels: 3,  // 默认使用Sv39x4 (3级页表)
            pa_max: 1usize << 41,  // 默认2048GB地址空间
            pa_bits: 41,  // Sv39x4支持最大41位物理地址
        }
    }

    fn init(mut self, vmdata: VmDataWeak) -> Result<Self::Inited, anyhow::Error>
    where
        Self: Sized,
    {
        self.init_raw(vmdata)
    }
}

impl VmMachineUninit {
    /// 根据地址范围确定合适的页表级数和模式
    /// 返回: (页表级数, 模式名称, 最大可寻址空间)
    fn determine_page_table_config(pa_max: usize) -> (usize, &'static str, usize) {
        const SV39X4_MAX: usize = 1usize << 41;  // 4x512GB
        const SV48X4_MAX: usize = 1usize << 50;  // 4x256TB
        const SV57X4_MAX: usize = 1usize << 59;  // 4x128PB
        
        if pa_max <= SV39X4_MAX {
            (3, "Sv39x4", SV39X4_MAX)
        } else if pa_max <= SV48X4_MAX {
            (4, "Sv48x4", SV48X4_MAX)
        } else if pa_max <= SV57X4_MAX {
            (5, "Sv57x4", SV57X4_MAX)
        } else {
            panic!("Address range {:#x} exceeds maximum supported by RISC-V virtualization (Sv57x4)", pa_max);
        }
    }

    /// 获取模式名称用于日志
    fn get_mode_name(pt_levels: usize) -> &'static str {
        match pt_levels {
            3 => "Sv39x4",
            4 => "Sv48x4",
            5 => "Sv57x4",
            _ => "Unknown",
        }
    }

    fn new_vcpus(&mut self, vm: &VmDataWeak) -> anyhow::Result<Vec<VCpu>> {
        // Create vCPUs
        let mut vcpus = vec![];

        let dtb_addr = GuestPhysAddr::from_usize(0);

        match self.config.cpu_num {
            CpuNumType::Alloc(num) => {
                for _ in 0..num {
                    let vcpu = VCpu::new(None, dtb_addr, vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
            CpuNumType::Fixed(ref ids) => {
                for id in ids {
                    let vcpu = VCpu::new(Some(*id), dtb_addr, vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
        }

        let vcpu_count = vcpus.len();

        // 从每个vCPU获取硬件能力限制
        for vcpu in &vcpus {
            let (max_levels, max_pa, pa_bits) = vcpu.with_hcpu(|cpu| {
                (
                    cpu.max_guest_page_table_levels(),
                    cpu.pa_range.end,
                    cpu.pa_bits,
                )
            });
            
            // 取所有vCPU中最小的能力值
            if max_levels < self.pt_levels {
                self.pt_levels = max_levels;
            }
            if max_pa < self.pa_max {
                self.pa_max = max_pa;
            }
            if pa_bits < self.pa_bits {
                self.pa_bits = pa_bits;
            }
        }

        // Safety clamp: PA should not exceed 56 bits for RISC-V Sv39x4/Sv48x4/Sv57x4
        // Two-stage translation (guest stage-2) supports up to 56-bit physical addresses
        const MAX_PA_BITS: usize = 56;
        const MAX_PA: usize = 1usize << MAX_PA_BITS;
        
        if self.pa_max > MAX_PA {
            log::warn!(
                "Clamping PA range from {:#x} to {:#x} (max supported for RISC-V two-stage translation)",
                self.pa_max,
                MAX_PA
            );
            self.pa_max = MAX_PA;
            self.pa_bits = self.pa_bits.min(MAX_PA_BITS);
        }

        // 确定合适的页表配置
        let (recommended_levels, mode_name, max_addressable) = 
            Self::determine_page_table_config(self.pa_max);
        
        if recommended_levels < self.pt_levels {
            log::info!(
                "Using {} ({}-level page table) for PA range {:#x} (max {:#x})",
                mode_name, recommended_levels, self.pa_max, max_addressable
            );
            self.pt_levels = recommended_levels;
        }

        debug!(
            "VM {} ({}) configuration:\n  vCPU count: {}\n  Page Table Mode: {}\n  Page Table Levels: {}\n  Max GPA: {:#x}\n  PA Bits: {}",
            self.config.id, 
            self.config.name, 
            vcpu_count, 
            Self::get_mode_name(self.pt_levels),
            self.pt_levels,
            self.pa_max, 
            self.pa_bits
        );
        
        Ok(vcpus)
    }

    fn init_raw(&mut self, vmdata: VmDataWeak) -> anyhow::Result<VmMachineInited> {
        debug!("Initializing VM {} ({})", self.config.id, self.config.name);
        let mut cpus = self.new_vcpus(&vmdata)?;

        // Ensure PA range covers all configured memory regions
        // Calculate the maximum GPA used by memory regions
        let mut max_gpa = 0;
        for memory_cfg in &self.config.memory_regions {
            use crate::config::MemoryKind;
            let (end_addr, base_addr) = match memory_cfg {
                MemoryKind::Identical { size } => {
                    // For Identical mapping, GPA == HPA, so we need the HPA
                    // which is calculated in new_memory. For now, assume it's
                    // at the same address as the virtual address we allocate.
                    // The actual mapping will be created in new_memory().
                    (*size, 0) // Will be updated in new_memory
                }
                MemoryKind::Reserved { hpa, size } => {
                    (hpa.as_usize() + size, hpa.as_usize())
                }
                MemoryKind::Vmem { gpa, size } => {
                    (gpa.as_usize() + size, gpa.as_usize())
                }
            };
            if end_addr > max_gpa {
                max_gpa = end_addr;
            }
        }

        // Also ensure PA range covers kernel load address
        let image_cfg = self.config.image_config();
        let kernel_end = if let Some(gpa) = image_cfg.kernel.gpa {
            gpa.as_usize() + image_cfg.kernel.data.len()
        } else {
            // If no GPA specified, kernel will be loaded into first memory region
            // at offset 2MB, so we need at least 2MB + kernel size
            2 * 1024 * 1024 + image_cfg.kernel.data.len()
        };

        let mut required_pa_max = max_gpa.max(kernel_end);
        
        // Round up to next power of two for better alignment
        // but ensure it doesn't exceed reasonable limits
        if required_pa_max > 0 {
            required_pa_max = required_pa_max.next_power_of_two();
        }

        // Use the maximum of vCPU PA range and required PA range
        if required_pa_max > self.pa_max {
            log::warn!(
                "Extending PA range from {:#x} to {:#x} to cover memory regions",
                self.pa_max,
                required_pa_max
            );
            self.pa_max = required_pa_max;

            // Recalculate page table levels based on new PA range
            let (new_pt_levels, new_mode_name, new_max_addressable) = 
                Self::determine_page_table_config(self.pa_max);
            
            if new_pt_levels > self.pt_levels {
                log::warn!(
                    "Increasing page table mode from {} to {} for extended PA range (max addressable: {:#x})",
                    Self::get_mode_name(self.pt_levels),
                    new_mode_name,
                    new_max_addressable
                );
                self.pt_levels = new_pt_levels;
            }
        }

        // 验证最终配置的有效性
        let (final_levels, final_mode, max_addressable) = 
            Self::determine_page_table_config(self.pa_max);
        
        if self.pt_levels < final_levels {
            return Err(anyhow::anyhow!(
                "Page table configuration error: {} requires {}-level page table, but only {} levels configured",
                final_mode, final_levels, self.pt_levels
            ));
        }
        
        if self.pa_max > max_addressable {
            return Err(anyhow::anyhow!(
                "PA range {:#x} exceeds maximum addressable space {:#x} for {} mode",
                self.pa_max, max_addressable, Self::get_mode_name(self.pt_levels)
            ));
        }

        debug!(
            "Creating VmAddrSpace: mode={}, pt_levels={}, pa_range=0..{:#x}",
            Self::get_mode_name(self.pt_levels),
            self.pt_levels, 
            self.pa_max
        );

        let mut vmspace = VmAddrSpace::new(
            self.pt_levels,
            GuestPhysAddr::from_usize(0)..self.pa_max.into(),
            self.config.emu_devices.clone(),
            self.config.cpu_num.num(),
        )?;

        debug!(
            "Mapping memory regions for VM {} ({})",
            self.config.id, self.config.name
        );
        for memory_cfg in &self.config.memory_regions {
            vmspace.new_memory(memory_cfg)?;
        }

        // Initialize VirtIO block device if host block device is available
        #[cfg(feature = "virtio-blk")]
        {
            debug!("Searching for VirtIO block device in {} emu_devices", self.config.emu_devices.len());
            for (i, dev) in self.config.emu_devices.iter().enumerate() {
                debug!("  Device {}: name={}, type={:?}, base_gpa={:#x}",
                    i, dev.name, dev.emu_type, dev.base_gpa);
            }

            // Find VirtIO block device config from emu_devices
            let virtio_blk_config = self.config.emu_devices.iter()
                .find(|dev| dev.emu_type == EmulatedDeviceType::VirtioBlk);

            debug!("VirtIO block config found: {}", virtio_blk_config.is_some());

            if let Some(config) = virtio_blk_config {
                // Try to get host block device from axruntime
                if let Some(mut blk_devs) = axruntime::take_block_devices() {
                    if let Some(host_dev) = blk_devs.take_one() {
                        info!(
                            "Initializing VirtIO block device at {:#x} with host device",
                            config.base_gpa
                        );

                        // QEMU virtio-mmio devices use IRQ 1-8
                        let irq_id = if config.irq_id > 0 { config.irq_id as u32 } else { 1 };

                        vmspace.init_virtio_blk(
                            host_dev,
                            config.base_gpa.into(),
                            config.length,
                            irq_id,
                        )?;
                    } else {
                        warn!("No host block device available for VirtIO block");
                    }
                } else {
                    warn!("Block devices not saved by axruntime (hv-blk feature not enabled?)");
                }
            }
        }

        // Initialize VirtIO console device if configured
        #[cfg(feature = "virtio-console")]
        {
            // Find VirtIO console device config from emu_devices
            let virtio_console_config = self.config.emu_devices.iter()
                .find(|dev| dev.emu_type == EmulatedDeviceTypeConsole::VirtioConsole);

            if let Some(config) = virtio_console_config {
                info!(
                    "Initializing VirtIO console device at {:#x}",
                    config.base_gpa
                );

                // QEMU virtio-mmio devices use IRQ 1-8, use irq_id from config or default to 2
                let irq_id = if config.irq_id > 0 { config.irq_id as u32 } else { 2 };

                vmspace.init_virtio_console(
                    config.base_gpa.into(),
                    config.length,
                    irq_id,
                )?;
            }
        }

        vmspace.load_kernel_image(&self.config)?;

        // Load DTB: prefer guest-provided DTB from config, fall back to building one from hypervisor DTB
        let dtb_addr = if let Some(ref dtb_config) = self.config.image_config.dtb {
            // Use the guest's pre-configured DTB (which includes UART, PLIC, VirtIO devices)
            info!(
                "Loading guest DTB from config: {} bytes",
                dtb_config.data.len()
            );
            vmspace.load_dtb(&dtb_config.data)?
        } else {
            // Fall back to building DTB from hypervisor's DTB (may lack device nodes)
            warn!("No guest DTB provided, building minimal DTB from hypervisor DTB");
            let mut fdt = FdtBuilder::new()?;
            fdt.setup_cpus(cpus.iter().map(|c| c.deref()))?;
            fdt.setup_memory(vmspace.memories().iter())?;
            fdt.setup_chosen(None)?;
            let dtb_data = fdt.build()?;

            vmspace.load_dtb(&dtb_data)?
        };

        // Map passthrough regions for devices (e.g., UART)
        // These are identity-mapped (GPA == HPA) so the guest can access them directly
        for pt_addr in &self.config.passthrough_addresses {
            vmspace.add_passthrough_mapping(
                GuestPhysAddr::from_usize(pt_addr.base_gpa),
                pt_addr.length,
            )?;
        }

        let kernel_entry = vmspace.kernel_entry();
        let gpt_root = vmspace.gpt_root();

        // Setup vCPUs
        for (i, vcpu) in cpus.iter_mut().enumerate() {
            // Set kernel entry point
            vcpu.vcpu
                .set_entry(kernel_entry)
                .map_err(|e| anyhow::anyhow!("Failed to set vCPU entry: {e:?}"))?;

            // RISC-V Linux boot protocol:
            // a0 = hart ID (hardware thread ID)
            // a1 = DTB pointer (device tree blob address)
            vcpu.vcpu
                .set_hart_id(i)
                .map_err(|e| anyhow::anyhow!("Failed to set hart ID: {e:?}"))?;
            
            vcpu.vcpu
                .set_dtb_addr(dtb_addr)
                .map_err(|e| anyhow::anyhow!("Failed to set DTB address: {e:?}"))?;

            // Configure page table parameters for two-stage translation
            vcpu.set_pt_level(self.pt_levels);
            vcpu.set_pa_bits(self.pa_bits);

            // Setup vCPU (including CSR initialization)
            vcpu.vcpu
                .setup()
                .map_err(|e| anyhow::anyhow!("Failed to setup vCPU: {e:?}"))?;

            // Set EPT (Extended Page Table) root for stage-2 translation
            // This sets the hgatp register with the correct MODE and PPN
            vcpu.vcpu
                .set_ept_root(gpt_root)
                .map_err(|e| anyhow::anyhow!("Failed to set EPT root for vCPU: {e:?}"))?;

            debug!(
                "Configured vCPU {}: hart_id={}, entry={:#x}, dtb={:#x}, mode={}",
                i, i, kernel_entry, dtb_addr, Self::get_mode_name(self.pt_levels)
            );
        }

        info!(
            "Successfully initialized VM {} ({}) with {} mode, {} vCPUs",
            self.config.id,
            self.config.name,
            Self::get_mode_name(self.pt_levels),
            cpus.len()
        );

        Ok(VmMachineInited {
            id: self.config.id.into(),
            name: self.config.name.clone(),
            vmspace,
            vcpus: cpus,
        })
    }
}
