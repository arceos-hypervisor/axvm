use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use axerrno::{AxResult, ax_err, ax_err_type};
use memory_addr::{align_up, is_aligned};
use page_table_multiarch::PageSize;
use spin::Mutex;

use axaddrspace::npt::{EPTEntry, EPTMetadata};
use axaddrspace::{AddrSpace, GuestPhysAddr, HostPhysAddr, MappingFlags};

use axdevice::{AxVmDeviceConfig, AxVmDevices};
use axvcpu::{AxArchVCpu, AxVCpu, AxVCpuExitReason, AxVCpuHal};

use crate::config::{AxVMConfig, VmMemMappingType};
use crate::vcpu::AxArchVCpuImpl;
use crate::{AxVMHal, has_hardware_support};

const VM_ASPACE_BASE: usize = 0x0;
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;

/// A vCPU with architecture-independent interface.
#[allow(type_alias_bounds)]
pub type VCpu<U: AxVCpuHal> = AxVCpu<AxArchVCpuImpl<U>>;
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
    is_host_vm: bool,
}

unsafe impl<U: AxVCpuHal> Send for AxVMInnerConst<U> {}
unsafe impl<U: AxVCpuHal> Sync for AxVMInnerConst<U> {}

struct AxVMInnerMut<H: AxVMHal> {
    // Todo: use more efficient lock.
    address_space: Mutex<AddrSpace<EPTMetadata, EPTEntry, H::PagingHandler>>,
    shm_region_base: Mutex<usize>,
    _marker: core::marker::PhantomData<H>,
}

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
                let arch_config = vcpu_id;
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
            // Find the end of guest VM's physical address space.
            let mut max_gpa = 0;
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

                if mem_region.gpa + mem_region.size > max_gpa {
                    max_gpa = mem_region.gpa + mem_region.size;
                }

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
                                true,
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

            for pt_device in config.pass_through_devices() {
                info!(
                    "Setting up passthrough device memory region: [{:#x}~{:#x}] -> [{:#x}~{:#x}]",
                    pt_device.base_gpa,
                    pt_device.base_gpa + pt_device.length,
                    pt_device.base_hpa,
                    pt_device.base_hpa + pt_device.length
                );

                address_space.map_linear(
                    GuestPhysAddr::from(pt_device.base_gpa),
                    HostPhysAddr::from(pt_device.base_hpa),
                    pt_device.length,
                    MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
                    false,
                )?;
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
                    is_host_vm: false,
                },
                inner_mut: AxVMInnerMut {
                    address_space: Mutex::new(address_space),
                    shm_region_base: Mutex::new(max_gpa + 0x1000), // Start from the next page after the max gpa.
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

    /// Returns the VM name.
    /// The name is used for logging and debugging purposes.
    /// It is not required to be unique.
    /// The name is set in the VM configuration.
    /// If the name is not set in the configuration, the name is an empty string.
    /// The name is immutable.
    #[inline]
    pub fn name(&self) -> String {
        self.inner_const.config.name()
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

    /// Returns if this VM is a host VM.
    #[inline]
    pub const fn is_host_vm(&self) -> bool {
        self.inner_const.is_host_vm
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

    pub fn translate_guest_memory_range(
        &self,
        gpa: GuestPhysAddr,
        size: usize,
    ) -> AxResult<Vec<(HostPhysAddr, usize)>> {
        let addr_space = self.inner_mut.address_space.lock();
        let translated = addr_space
            .translate_range(gpa, size)
            .ok_or_else(|| ax_err_type!(NotFound, "Failed to translate guest memory range"))?;
        Ok(translated)
    }

    pub fn alloc_one_shm_region(&self, size: usize, alignment: usize) -> AxResult<GuestPhysAddr> {
        if !is_aligned(size, alignment) {
            error!("Size {:#x} is not aligned to {:#x}", size, alignment);
            return ax_err!(InvalidInput, "Size is not aligned");
        }

        let mut shm_region_base = self.inner_mut.shm_region_base.lock();
        let base_unaligned = *shm_region_base;
        let base_aligned = align_up(base_unaligned, alignment);
        if base_aligned + size > VM_ASPACE_SIZE {
            return ax_err!(NoMemory, "No more shared memory region available");
        }
        *shm_region_base = base_aligned + size;

        debug!(
            "Allocating shared memory region: [{:#x}~{:#x}], shm_base extend to {:#x}",
            base_aligned,
            base_aligned + size,
            *shm_region_base
        );

        Ok(GuestPhysAddr::from(base_aligned))
    }

    /// Translates a guest physical address to a host physical address.
    /// Returns None if the translation fails or the address is not mapped.
    pub fn guest_phys_to_host_phys(
        &self,
        gpa: GuestPhysAddr,
    ) -> Option<(HostPhysAddr, MappingFlags, PageSize)> {
        self.inner_mut.address_space.lock().translate(gpa)
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
                    let val = self
                        .get_devices()
                        .handle_mmio_read(*addr, (*width).into())?;
                    vcpu.set_gpr(*reg, val);
                    true
                }
                AxVCpuExitReason::MmioWrite { addr, width, data } => {
                    self.get_devices()
                        .handle_mmio_write(*addr, (*width).into(), *data as usize);
                    true
                }
                AxVCpuExitReason::IoRead { port: _, width: _ } => true,
                AxVCpuExitReason::IoWrite {
                    port: _,
                    width: _,
                    data: _,
                } => true,
                AxVCpuExitReason::NestedPageFault { addr, access_flags } => self
                    .inner_mut
                    .address_space
                    .lock()
                    .handle_page_fault(*addr, *access_flags),
                AxVCpuExitReason::Nothing => {
                    if self.is_host_vm() {
                        // Host VM does not need to handle Nothing exit reason.
                        // Because its vcpus will not be scheduled among different physical CPUs.
                        // Just continue running to avoid redundant `unbind` and `bind` operations.
                        true
                    } else {
                        // To allow the scheduler to have a chance to check scheduling status.
                        false
                    }
                }
                _ => false,
            };
            if !handled {
                break exit_reason;
            }
        };

        vcpu.unbind()?;
        Ok(exit_reason)
    }
}

use x86_vcpu::LinuxContext;

impl<H: AxVMHal, U: AxVCpuHal> AxVM<H, U> {
    pub fn new_host(config: AxVMConfig, host_ctxs: &[LinuxContext]) -> AxResult<AxVMRef<H, U>> {
        let result = Arc::new({
            // Set up Memory regions.
            let mut address_space =
                AddrSpace::new_empty(GuestPhysAddr::from(VM_ASPACE_BASE), VM_ASPACE_SIZE)?;
            // Find the end of guest VM's physical address space.
            let mut max_gpa = 0;
            for mem_region in config.memory_regions() {
                let mapping_flags = MappingFlags::from_bits(mem_region.flags).ok_or_else(|| {
                    ax_err_type!(
                        InvalidInput,
                        format!("Illegal flags {:?}", mem_region.flags)
                    )
                })?;

                info!(
                    "Setting up host VM memory region: [{:#x}~{:#x}] {:?}",
                    mem_region.gpa,
                    mem_region.gpa + mem_region.size,
                    mapping_flags
                );

                if mem_region.gpa + mem_region.size > max_gpa {
                    max_gpa = mem_region.gpa + mem_region.size;
                }

                // Handle ram region.
                match mem_region.map_type {
                    VmMemMappingType::MapIentical => {
                        address_space.map_linear(
                            GuestPhysAddr::from(mem_region.gpa),
                            HostPhysAddr::from(mem_region.gpa),
                            mem_region.size,
                            mapping_flags,
                            true,
                        )?;
                    }
                    VmMemMappingType::MapAlloc => {
                        warn!("MapAlloc is not supported for host VM");
                    }
                }
            }

            let devices = axdevice::AxVmDevices::new(AxVmDeviceConfig {
                emu_configs: config.emu_devices().to_vec(),
            });

            let vcpu_id_pcpu_sets = config.get_vcpu_affinities_pcpu_ids();

            // Create VCpus.
            let mut vcpu_list = Vec::with_capacity(vcpu_id_pcpu_sets.len());

            for (vcpu_id, phys_cpu_set, _pcpu_id) in vcpu_id_pcpu_sets {
                debug!("Creating host vCPU[{}] {:x?}", vcpu_id, phys_cpu_set,);
                let vcpu = VCpu::new(vcpu_id, 0, phys_cpu_set, vcpu_id)?;

                // Setup VCpus.
                vcpu.setup_from_context(
                    address_space.page_table_root(),
                    host_ctxs[vcpu_id].clone(),
                )?;

                vcpu_list.push(Arc::new(vcpu));
            }

            Self {
                running: AtomicBool::new(false),
                inner_const: AxVMInnerConst {
                    id: 0,
                    config,
                    vcpu_list: vcpu_list.into_boxed_slice(),
                    devices,
                    is_host_vm: true,
                },
                inner_mut: AxVMInnerMut {
                    address_space: Mutex::new(address_space),
                    shm_region_base: Mutex::new(max_gpa + 0x1000), // Start from the next page after the max gpa.
                    _marker: core::marker::PhantomData,
                },
            }
        });

        info!("Host VM created: id={}", result.id());

        Ok(result)
    }

    pub fn map_region(
        &self,
        gpa: GuestPhysAddr,
        hpa: HostPhysAddr,
        size: usize,
        flags: MappingFlags,
        allow_huge: bool,
    ) -> AxResult<()> {
        self.inner_mut
            .address_space
            .lock()
            .map_linear(gpa, hpa, size, flags, allow_huge)
    }

    pub fn unmap_region(&self, gpa: GuestPhysAddr, size: usize) -> AxResult<()> {
        self.inner_mut.address_space.lock().unmap(gpa, size)
    }

    pub fn read_from_guest_of<T>(&self, gpa_ptr: GuestPhysAddr) -> AxResult<T> {
        let addr_space = self.inner_mut.address_space.lock();

        match addr_space.translated_byte_buffer(gpa_ptr, core::mem::size_of::<T>()) {
            Some(buffer) => {
                if buffer.len() != 1 {
                    return ax_err!(InvalidInput, "Buffer is not contiguous");
                }

                let bytes = unsafe {
                    core::slice::from_raw_parts(buffer[0].as_ptr(), core::mem::size_of::<T>())
                };
                let data: T = unsafe { core::ptr::read(bytes.as_ptr() as *const T) };
                Ok(data)
            }
            None => ax_err!(InvalidInput, "Failed to translate guest physical address"),
        }
    }

    pub fn read_from_guest_of_slice<T>(
        &self,
        gpa_ptr: GuestPhysAddr,
        count: usize,
    ) -> AxResult<Vec<T>> {
        let addr_space = self.inner_mut.address_space.lock();

        match addr_space.translated_byte_buffer(gpa_ptr, core::mem::size_of::<T>() * count) {
            Some(buffer) => {
                if buffer.len() != 1 {
                    return ax_err!(InvalidInput, "Buffer is not contiguous");
                }
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        buffer[0].as_ptr(),
                        core::mem::size_of::<T>() * count,
                    )
                };
                let mut data = Vec::with_capacity(count);
                for i in 0..count {
                    let item: T = unsafe {
                        core::ptr::read(
                            bytes.as_ptr().add(i * core::mem::size_of::<T>()) as *const T
                        )
                    };
                    data.push(item);
                }
                Ok(data)
            }
            None => ax_err!(InvalidInput, "Failed to translate guest physical address"),
        }
    }

    pub fn write_to_guest_of<T>(&self, gpa_ptr: GuestPhysAddr, data: &T) -> AxResult {
        let addr_space = self.inner_mut.address_space.lock();

        match addr_space.translated_byte_buffer(gpa_ptr, core::mem::size_of::<T>()) {
            Some(mut buffer) => {
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        data as *const T as *const u8,
                        core::mem::size_of::<T>(),
                    )
                };
                let mut copied_bytes = 0;
                for (_i, chunk) in buffer.iter_mut().enumerate() {
                    let end = copied_bytes + chunk.len();
                    chunk.copy_from_slice(&bytes[copied_bytes..end]);
                    copied_bytes += chunk.len();
                }
                Ok(())
            }
            None => ax_err!(InvalidInput, "Failed to translate guest physical address"),
        }
    }
}
