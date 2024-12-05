use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use axaddrspace::device::AccessWidth;
use axdevice_base::DeviceRWContext;
use cpumask::CpuMask;
// use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use axdevice::{AxVmDeviceConfig, AxVmDevices};
use axerrno::{ax_err, ax_err_type, AxResult};
use spin::Mutex;

use axvcpu::{AxArchVCpu, AxVCpu, AxVCpuExitReason, AxVCpuHal};

use axaddrspace::{AddrSpace, GuestPhysAddr, HostPhysAddr, MappingFlags};

use crate::config::AxVMConfig;
use crate::vcpu::{AxArchVCpuImpl, AxVCpuCreateConfig};
use crate::{has_hardware_support, AxVMHal};

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
    inner_const: AxVMInnerConst<U>,
    inner_mut: AxVMInnerMut<H>,
}

impl<H: AxVMHal, U: AxVCpuHal> AxVM<H, U> {
    /// Creates a new VM with the given configuration.
    /// Returns an error if the configuration is invalid.
    /// The VM is not started until `boot` is called.
    pub fn new(config: AxVMConfig) -> AxResult<AxVMRef<H, U>> {
        let result = Arc::new({
            let vcpu_id_pcpu_sets = config.get_vcpu_affinities_pcpu_ids();

            // Create VCpus.
            let mut vcpu_list = Vec::with_capacity(vcpu_id_pcpu_sets.len());

            for (vcpu_id, phys_cpu_set, _pcpu_id) in vcpu_id_pcpu_sets {
                #[cfg(target_arch = "aarch64")]
                let arch_config = AxVCpuCreateConfig {
                    mpidr_el1: _pcpu_id as _,
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

            // Set up Memory regions.
            let mut address_space =
                AddrSpace::new_empty(GuestPhysAddr::from(VM_ASPACE_BASE), VM_ASPACE_SIZE)?;
            for mem_region in config.memory_regions() {
                let mapping_flags = MappingFlags::from_bits(mem_region.flags).ok_or_else(|| {
                    ax_err_type!(
                        InvalidInput,
                        format!("Illegal flags {:?}", mem_region.flags)
                    )
                })?;

                // Handle passthrough device's memory region.
                // Todo: Perhaps we can merge the management of passthrough device memory
                //       into the device configuration file.
                if mapping_flags.contains(MappingFlags::DEVICE) {
                    address_space.map_linear(
                        GuestPhysAddr::from(mem_region.gpa),
                        HostPhysAddr::from(mem_region.gpa),
                        mem_region.size,
                        mapping_flags,
                    )?;
                } else {
                    // Handle ram region.
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

            let devices = axdevice::AxVmDevices::new(AxVmDeviceConfig {
                emu_configs: config.emu_devices().to_vec(),
            });

            Self {
                running: AtomicBool::new(false),
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
            }
        });

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
            ax_err!(BadState, format!("VM[{}] is running", self.id()))
        } else {
            info!("Booting VM[{}]", self.id());
            self.running.store(true, Ordering::Relaxed);
            Ok(())
        }
    }

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
                    let val = self.get_devices().handle_mmio_read(
                        *addr,
                        (*width).into(),
                        DeviceRWContext::new(vcpu_id),
                    )?;
                    vcpu.set_gpr(*reg, val);
                    true
                }
                AxVCpuExitReason::MmioWrite { addr, width, data } => {
                    self.get_devices().handle_mmio_write(
                        *addr,
                        (*width).into(),
                        *data as usize,
                        DeviceRWContext::new(vcpu_id),
                    )?;
                    true
                }
                AxVCpuExitReason::IoRead { port, width } => {
                    let val = self.get_devices().handle_port_read(
                        *port,
                        *width,
                        DeviceRWContext::new(vcpu_id),
                    )?;
                    vcpu.set_gpr(0, val); // The target is always eax/ax/al, todo: handle access_width correctly

                    true
                }
                AxVCpuExitReason::IoWrite { port, width, data } => {
                    self.get_devices().handle_port_write(
                        *port,
                        *width,
                        *data as usize,
                        DeviceRWContext::new(vcpu_id),
                    )?;
                    true
                }
                AxVCpuExitReason::SysRegRead { addr, reg } => {
                    let val = self.get_devices().handle_sys_reg_read(
                        *addr,
                        // Generally speaking, the width of system register is fixed and needless to be specified.
                        // AccessWidth::Qword here is just a placeholder, may be changed in the future.
                        AccessWidth::Qword,
                        DeviceRWContext::new(vcpu_id),
                    )?;
                    vcpu.set_gpr(*reg, val);
                    true
                }
                AxVCpuExitReason::SysRegWrite { addr, value } => {
                    self.get_devices().handle_sys_reg_write(
                        *addr,
                        AccessWidth::Qword,
                        *value as usize,
                        DeviceRWContext::new(vcpu_id),
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
        // Check if the current running vm is self.
        //
        // It is not supported to inject interrupt to a vcpu in another VM yet.
        //
        // It may be supported in the future, as a essential feature for cross-VM communication.
        if H::current_vm_id() != self.id() {
            panic!("Injecting interrupt to a vcpu in another VM is not supported");
        }

        let current_vcpu_id = H::current_vcpu_id();
        let current_pcpu_id = H::current_pcpu_id();

        for target_vcpu in &targets {
            let target_vcpu = self.vcpu(target_vcpu).ok_or_else(|| {
                ax_err_type!(InvalidInput, format!("Invalid vcpu_id {}", target_vcpu))
            })?;

            if target_vcpu.id() == current_vcpu_id {
                target_vcpu.inject_interrupt(irq)?;
            } else {
                let target_pcpu_id = self.where_is_vcpu(&target_vcpu)?;

                if target_pcpu_id == current_pcpu_id {
                    target_vcpu.inject_interrupt(irq)?; // <- Maybe another method to queue the interrupt?
                } else {
                }
            }
        }

        Ok(())
    }

    /// Returns on which physical CPU the vCPU is running/waiting/queueing.
    pub fn where_is_vcpu(&self, _vcpu: &AxVCpuRef<U>) -> AxResult<usize> {
        todo!()
    }

    /// Injects an interrupt to a vCPU which is on another physical CPU.
    pub fn inject_interrupt_to_vcpu_remotely(
        &self,
        _vcpu_id: usize,
        _irq: usize,
        _pcpu_id: usize,
    ) -> AxResult {
        todo!()
    }
}
