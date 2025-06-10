use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::vec;
// use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use memory_addr::{align_down_4k, align_up_4k};

use axerrno::{AxResult, ax_err, ax_err_type};
use spin::Mutex;

use axaddrspace::{AddrSpace, GuestPhysAddr, HostPhysAddr, MappingFlags, device::AccessWidth};
use axdevice::{AxVmDeviceConfig, AxVmDevices};
use axvcpu::{AxArchVCpu, AxVCpu, AxVCpuExitReason, AxVCpuHal};
use cpumask::CpuMask;

use crate::config::{AxVMConfig, VmMemMappingType};
use crate::vcpu::{AxArchVCpuImpl, AxVCpuCreateConfig};
use crate::{AxVMHal, has_hardware_support};

#[cfg(target_arch = "aarch64")]
use crate::vcpu::{get_sysreg_device};

const VM_ASPACE_BASE: usize = 0x0;
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;

/// A vCPU with architecture-independent interface.
#[allow(type_alias_bounds)]
type VCpu<U: AxVCpuHal> = AxVCpu<AxArchVCpuImpl<U>>;
/// A reference to a vCPU.
#[allow(type_alias_bounds)]
pub type AxVCpuRef<U: AxVCpuHal> = Arc<VCpu<U>>;
/// A reference to a VM.
#[allow(type_alias_bounds)]
pub type AxVMRef<H: AxVMHal, U: AxVCpuHal> = Arc<AxVM<H, U>>; // we know the bound is not enforced here, we keep it for clarity

struct AxVMInnerConst<U: AxVCpuHal> {
    id: usize,
    config: AxVMConfig,
    vcpu_list: Box<[AxVCpuRef<U>]>,
    devices: AxVmDevices,
}

unsafe impl<U: AxVCpuHal> Send for AxVMInnerConst<U> {}
unsafe impl<U: AxVCpuHal> Sync for AxVMInnerConst<U> {}

struct AxVMInnerMut<H: AxVMHal> {
    // Todo: use more efficient lock.
    address_space: Mutex<AddrSpace<H::PagingHandler>>,
    _marker: core::marker::PhantomData<H>,
}

const TEMP_MAX_VCPU_NUM: usize = 64;

/// A Virtual Machine.
pub struct AxVM<H: AxVMHal, U: AxVCpuHal> {
    running: AtomicBool,
    shutting_down: AtomicBool,
    inner_const: AxVMInnerConst<U>,
    inner_mut: AxVMInnerMut<H>,
}

impl<H: AxVMHal, U: AxVCpuHal> AxVM<H, U> {
    fn new_without_setup(config: AxVMConfig) -> AxResult<AxVM<H, U>> {
        let vcpu_id_pcpu_sets = config.get_vcpu_affinities_pcpu_ids();
        debug!("id: {}, VCpuIdPCpuSets: {:#x?}", config.id(), vcpu_id_pcpu_sets);
        let mut vcpu_list = Vec::with_capacity(vcpu_id_pcpu_sets.len());
        for (vcpu_id, phys_cpu_set, _pcpu_id) in vcpu_id_pcpu_sets {
            #[cfg(target_arch = "aarch64")]
            let arch_config = AxVCpuCreateConfig {
                mpidr_el1: _pcpu_id as _,
                dtb_addr: config
                    .image_config()
                    .dtb_load_gpa
                    .unwrap_or_default()
                    .as_usize(),
            };
            #[cfg(target_arch = "riscv64")]
            let arch_config = AxVCpuCreateConfig {
                hart_id: vcpu_id as _,
                dtb_addr: config
                    .image_config()
                    .dtb_load_gpa
                    .unwrap_or(GuestPhysAddr::from_usize(0x9000_0000)),
            };
            #[cfg(target_arch = "x86_64")]
            let arch_config = AxVCpuCreateConfig::default();

            vcpu_list.push(Arc::new(VCpu::new(
                vcpu_id,
                0, // Currently not used.
                phys_cpu_set,
                arch_config,
            )?));
        }
        let mut address_space =
            AddrSpace::new_empty(GuestPhysAddr::from(VM_ASPACE_BASE), VM_ASPACE_SIZE)?;
        for mem_region in config.memory_regions() {
            let mapping_flags = MappingFlags::from_bits(mem_region.flags).ok_or_else(|| {
                ax_err_type!(
                    InvalidInput,
                    format!("Illegal flags {:?}", mem_region.flags)
                )
            })?;

            // Check mapping flags.
            if mapping_flags.contains(MappingFlags::DEVICE) {
                warn!(
                    "Do not include DEVICE flag in memory region flags, it should be configured in pass_through_devices"
                );
                continue;
            }

            info!(
                "Setting up memory region: [{:#x}~{:#x}] {:?}",
                mem_region.gpa,
                mem_region.gpa + mem_region.size,
                mapping_flags
            );

            // Handle ram region.
            match mem_region.map_type {
                VmMemMappingType::MapIentical => {
                    if H::alloc_memory_region_at(
                        HostPhysAddr::from(mem_region.gpa),
                        mem_region.size,
                    ) {
                        address_space.map_linear(
                            GuestPhysAddr::from(mem_region.gpa),
                            HostPhysAddr::from(mem_region.gpa),
                            mem_region.size,
                            mapping_flags,
                        )?;
                    } else {
                        warn!(
                            "Failed to allocate memory region at {:#x} for VM [{}]",
                            mem_region.gpa,
                            config.id()
                        );
                    }
                }
                VmMemMappingType::MapAlloc => {
                    // Note: currently we use `map_alloc`,
                    // which allocates real physical memory in units of physical page frames,
                    // which may not be contiguous!!!
                    address_space.map_alloc(
                        GuestPhysAddr::from(mem_region.gpa),
                        mem_region.size,
                        mapping_flags,
                        true,
                    )?;
                }
            }
        }

        let mut pt_dev_region = Vec::new();
        for pt_device in config.pass_through_devices() {
            info!(
                "PT dev {:?} region: [{:#x}~{:#x}] -> [{:#x}~{:#x}]",
                pt_device.name,
                pt_device.base_gpa,
                pt_device.base_gpa + pt_device.length,
                pt_device.base_hpa,
                pt_device.base_hpa + pt_device.length
            );
            // Align the base address and length to 4K boundaries.
            pt_dev_region.push((
                align_down_4k(pt_device.base_gpa),
                align_up_4k(pt_device.length),
            ));
        }

        pt_dev_region.sort_by_key(|(gpa, _)| *gpa);

        // Merge overlapping regions.
        let pt_dev_region =
            pt_dev_region
                .into_iter()
                .fold(Vec::<(usize, usize)>::new(), |mut acc, (gpa, len)| {
                    if let Some(last) = acc.last_mut() {
                        if last.0 + last.1 >= gpa {
                            // Merge with the last region.
                            last.1 = (last.0 + last.1).max(gpa + len) - last.0;
                        } else {
                            acc.push((gpa, len));
                        }
                    } else {
                        acc.push((gpa, len));
                    }
                    acc
                });

        for (gpa, len) in &pt_dev_region {
            address_space.map_linear(
                GuestPhysAddr::from(*gpa),
                HostPhysAddr::from(*gpa),
                *len,
                MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
            )?;
        }

        let mut devices = axdevice::AxVmDevices::new(AxVmDeviceConfig {
            emu_configs: config.emu_devices().to_vec(),
        });

        #[cfg(target_arch = "aarch64")]
        {
            use arm_vcpu::gic::*;
            let qemu_configs_linux0 = GicDeviceConfig {
                gicd_base: 0x0800_0000.into(),
                gicrs: vec![GicDistributorConfig {
                    gicr_base: 0x080a_0000.into(),
                    cpu_id: 0, // For logging purposes only.
                }],
                assigned_spis: vec![
                    GicSpiAssignment {
                        spi: 0x28,
                        target_cpu_phys_id: 0,
                        target_cpu_affinity: (0, 0, 0, 0),
                    },
                ],
                gits_base: 0x0808_0000.into(),
                gits_phys_base: 0x0808_0000.into(),
                is_root_vm: false,
            };

            let qemu_configs_linux1 = GicDeviceConfig {
                gicd_base: 0x0800_0000.into(),
                gicrs: vec![GicDistributorConfig {
                    gicr_base: 0x080c_0000.into(),
                    cpu_id: 1, // For logging purposes only.
                }],
                assigned_spis: vec![
                    GicSpiAssignment {
                        spi: 0x2a,
                        target_cpu_phys_id: 1,
                        target_cpu_affinity: (0, 0, 0, 1),
                    },
                ],
                gits_base: 0x0808_0000.into(),
                gits_phys_base: 0x0808_0000.into(),
                is_root_vm: false,
            };

            for gic in get_gic_devices(if config.id() == 1 {
                qemu_configs_linux0
            } else {
                qemu_configs_linux1
            }) {
                debug!("Adding GIC device @ {:#x}", gic.address_range());
                devices.add_mmio_dev(gic);
            }
        }

        Ok(Self {
            running: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            inner_const: AxVMInnerConst {
                id: config.id(),
                config,
                vcpu_list: vcpu_list.into_boxed_slice(),
                devices,
            },
            inner_mut: AxVMInnerMut {
                address_space: Mutex::new(address_space),
                _marker: core::marker::PhantomData,
            },
        })
    }

    /// Creates a new VM with the given configuration.
    /// Returns an error if the configuration is invalid.
    /// The VM is not started until `boot` is called.
    pub fn new(config: AxVMConfig) -> AxResult<AxVMRef<H, U>> {
        let result = Arc::new(Self::new_without_setup(config)?);

        info!("VM created: id={}", result.id());

        // Setup VCpus.
        for vcpu in result.vcpu_list() {
            let entry = if vcpu.id() == 0 {
                result.inner_const.config.bsp_entry()
            } else {
                result.inner_const.config.ap_entry()
            };
            vcpu.setup(
                entry,
                result.ept_root(),
                <AxArchVCpuImpl<U> as AxArchVCpu>::SetupConfig::default(),
            )?;
        }
        info!("VM setup: id={}", result.id());

        Ok(result)
    }

    pub fn temp_new_with_device_adder(
        config: AxVMConfig,
        device_adder: impl FnOnce(&mut AxVmDevices),
    ) -> AxResult<AxVMRef<H, U>> {
        let mut result = Self::new_without_setup(config)?;

        device_adder(&mut result.inner_const.devices);

        let result = Arc::new(result);
        info!("VM created: id={}", result.id());

        // Setup VCpus.
        for vcpu in result.vcpu_list() {
            let entry = if vcpu.id() == 0 {
                result.inner_const.config.bsp_entry()
            } else {
                result.inner_const.config.ap_entry()
            };
            vcpu.setup(
                entry,
                result.ept_root(),
                <AxArchVCpuImpl<U> as AxArchVCpu>::SetupConfig::default(),
            )?;
        }
        info!("VM[{}] vcpus set up", result.id());

        Ok(result)
    }

    /// Returns the VM id.
    #[inline]
    pub const fn id(&self) -> usize {
        self.inner_const.id
    }

    /// Retrieves the vCPU corresponding to the given vcpu_id for the VM.
    /// Returns None if the vCPU does not exist.
    #[inline]
    pub fn vcpu(&self, vcpu_id: usize) -> Option<AxVCpuRef<U>> {
        self.vcpu_list().get(vcpu_id).cloned()
    }

    /// Returns the number of vCPUs corresponding to the VM.
    #[inline]
    pub const fn vcpu_num(&self) -> usize {
        self.inner_const.vcpu_list.len()
    }

    /// Returns a reference to the list of vCPUs corresponding to the VM.
    #[inline]
    pub fn vcpu_list(&self) -> &[AxVCpuRef<U>] {
        &self.inner_const.vcpu_list
    }

    /// Returns the base address of the two-stage address translation page table for the VM.
    pub fn ept_root(&self) -> HostPhysAddr {
        self.inner_mut.address_space.lock().page_table_root()
    }

    /// Returns guest VM image load region in `Vec<&'static mut [u8]>`,
    /// according to the given `image_load_gpa` and `image_size.
    /// `Vec<&'static mut [u8]>` is a series of (HVA) address segments,
    /// which may correspond to non-contiguous physical addresses,
    ///
    /// FIXME:
    /// Find a more elegant way to manage potentially non-contiguous physical memory
    ///         instead of `Vec<&'static mut [u8]>`.
    pub fn get_image_load_region(
        &self,
        image_load_gpa: GuestPhysAddr,
        image_size: usize,
    ) -> AxResult<Vec<&'static mut [u8]>> {
        let addr_space = self.inner_mut.address_space.lock();
        let image_load_hva = addr_space
            .translated_byte_buffer(image_load_gpa, image_size)
            .expect("Failed to translate kernel image load address");
        Ok(image_load_hva)
    }

    /// Returns if the VM is running.
    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Boots the VM by setting the running flag as true.
    pub fn boot(&self) -> AxResult {
        if !has_hardware_support() {
            ax_err!(Unsupported, "Hardware does not support virtualization")
        } else if self.running() {
            ax_err!(BadState, format!("VM[{}] is already running", self.id()))
        } else {
            info!("Booting VM[{}]", self.id());
            self.running.store(true, Ordering::Relaxed);
            Ok(())
        }
    }

    /// Returns if the VM is shutting down.
    pub fn shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Relaxed)
    }

    /// Shuts down the VM by setting the shutting_down flag as true.
    ///
    /// Currently, the "re-init" process of the VM is not implemented. Therefore, a VM can only be
    /// booted once. And after the VM is shut down, it cannot be booted again.
    pub fn shutdown(&self) -> AxResult {
        if self.shutting_down() {
            ax_err!(
                BadState,
                format!("VM[{}] is already shutting down", self.id())
            )
        } else {
            info!("Shutting down VM[{}]", self.id());
            self.shutting_down.store(true, Ordering::Relaxed);
            Ok(())
        }
    }

    // TODO: implement suspend/resume.
    // TODO: implement re-init.

    /// Returns this VM's emulated devices.
    pub fn get_devices(&self) -> &AxVmDevices {
        &self.inner_const.devices
    }

    /// Run a vCPU according to the given vcpu_id.
    ///
    /// ## Arguments
    /// * `vcpu_id` - the id of the vCPU to run.
    ///
    /// ## Returns
    /// * `AxVCpuExitReason` - the exit reason of the vCPU, wrapped in an `AxResult`.
    ///
    pub fn run_vcpu(&self, vcpu_id: usize) -> AxResult<AxVCpuExitReason> {
        let vcpu = self
            .vcpu(vcpu_id)
            .ok_or_else(|| ax_err_type!(InvalidInput, "Invalid vcpu_id"))?;

        vcpu.bind()?;

        let exit_reason = loop {
            let exit_reason = vcpu.run()?;
            trace!("{exit_reason:#x?}");
            let handled = match &exit_reason {
                AxVCpuExitReason::MmioRead {
                    addr,
                    width,
                    reg,
                    reg_width: _,
                } => {
                    let val = self
                        .get_devices()
                        .handle_mmio_read(*addr, (*width).into())?;
                    vcpu.set_gpr(*reg, val);
                    true
                }
                AxVCpuExitReason::MmioWrite { addr, width, data } => {
                    self.get_devices()
                        .handle_mmio_write(*addr, (*width).into(), *data as usize)?;
                    true
                }
                AxVCpuExitReason::IoRead { port, width } => {
                    let val = self.get_devices().handle_port_read(*port, *width)?;
                    vcpu.set_gpr(0, val); // The target is always eax/ax/al, todo: handle access_width correctly

                    true
                }
                AxVCpuExitReason::IoWrite { port, width, data } => {
                    self.get_devices()
                        .handle_port_write(*port, *width, *data as usize)?;
                    true
                }
                AxVCpuExitReason::SysRegRead { addr, reg } => {
                    let val = self.get_devices().handle_sys_reg_read(
                        *addr,
                        // Generally speaking, the width of system register is fixed and needless to be specified.
                        // AccessWidth::Qword here is just a placeholder, may be changed in the future.
                        AccessWidth::Qword,
                    )?;
                    vcpu.set_gpr(*reg, val);
                    true
                }
                AxVCpuExitReason::SysRegWrite { addr, value } => {
                    self.get_devices().handle_sys_reg_write(
                        *addr,
                        AccessWidth::Qword,
                        *value as usize,
                    )?;
                    true
                }
                AxVCpuExitReason::NestedPageFault { addr, access_flags } => self
                    .inner_mut
                    .address_space
                    .lock()
                    .handle_page_fault(*addr, *access_flags),
                _ => false,
            };
            if !handled {
                break exit_reason;
            }
        };

        vcpu.unbind()?;
        Ok(exit_reason)
    }

    /// Injects an interrupt to the vCPU.
    pub fn inject_interrupt_to_vcpu(
        &self,
        targets: CpuMask<TEMP_MAX_VCPU_NUM>,
        irq: usize,
    ) -> AxResult {
        let vm_id = self.id();
        // Check if the current running vm is self.
        //
        // It is not supported to inject interrupt to a vcpu in another VM yet.
        //
        // It may be supported in the future, as a essential feature for cross-VM communication.
        if H::current_vm_id() != self.id() {
            panic!("Injecting interrupt to a vcpu in another VM is not supported");
        }

        for target_vcpu in &targets {
            H::inject_irq_to_vcpu(vm_id, target_vcpu, irq)?;
        }

        Ok(())
    }

    /// Returns a reference to the VM's configuration.
    pub fn config(&self) -> &AxVMConfig {
        &self.inner_const.config
    }
}
