use alloc::vec::Vec;
use axaddrspace::{MappingFlags, GuestMemoryAccessor};
use core::{
    alloc::Layout,
    ops::{Deref, DerefMut, Range},
    fmt::Display,
};
use memory_addr::{MemoryAddr, PhysAddr};
use std::sync::{Arc, Mutex};

use ranges_ext::RangeInfo;

use axdevice::{AxVmDevices, AxVmDeviceConfig};
#[cfg(feature = "virtio-console")]
use axdevice::ConsoleInputHandler;
use axvmconfig::EmulatedDeviceConfig;

use crate::{
    AxVMConfig, GuestPhysAddr, HostPhysAddr, HostVirtAddr,
    config::MemoryKind,
    hal::{ArchOp, phys_to_virt, virt_to_phys},
};

const ALIGN: usize = 1024 * 1024 * 2;

type AddrSpaceRaw = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;
type AddrSpaceSync = Arc<Mutex<AddrSpaceRaw>>;

pub(crate) type VmRegionMap = ranges_ext::RangeSetAlloc<VmRegion>;

pub struct VmAddrSpace {
    pub aspace: AddrSpaceSync,
    pub region_map: VmRegionMap,
    pub devices: AxVmDevices,
    kernel_entry: GuestPhysAddr,
    kernel_memory_index: usize,
    memories: Vec<GuestMemory>,
    addr_space_end: GuestPhysAddr,
}

impl VmAddrSpace {
    pub fn new(
        gpt_levels: usize,
        vm_addr_space: Range<GuestPhysAddr>,
        emu_device_configs: Vec<EmulatedDeviceConfig>,
        cpu_count: usize,
    ) -> anyhow::Result<Self> {
        let region_map = VmRegionMap::new(Vec::new());
        let vm_space_size = vm_addr_space.end.as_usize() - vm_addr_space.start.as_usize();
        // Don't pre-add Passthrough region; it will be handled by map_passthrough_regions
        // Create address space for the VM
        let address_space = AddrSpaceRaw::new_empty(
            gpt_levels,
            vm_addr_space.start.as_usize().into(),
            vm_space_size,
        )
        .map_err(|e| anyhow!("Failed to create address space: {e:?}"))?;

        // Initialize device manager
        let device_config = AxVmDeviceConfig::new(emu_device_configs);
        let mut devices = AxVmDevices::new(device_config);

        // Initialize interrupt manager
        devices.init_interrupt_manager(cpu_count);

        Ok(Self {
            aspace: Arc::new(Mutex::new(address_space)),
            region_map,
            devices,
            kernel_entry: GuestPhysAddr::from_usize(0),
            kernel_memory_index: 0,
            memories: vec![],
            addr_space_end: vm_addr_space.end,
        })
    }

    pub fn gpt_root(&self) -> HostPhysAddr {
        let g = self.aspace.lock();
        g.page_table_root().as_usize().into()
    }

    pub fn kernel_entry(&self) -> GuestPhysAddr {
        self.kernel_entry
    }

    pub fn new_memory(&mut self, kind: &MemoryKind) -> anyhow::Result<()> {
        let _gpa;
        let _hpa;
        let _size;
        let _align = 0x1000;
        let _payload;
        let flags =
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE | MappingFlags::USER;

        match kind {
            MemoryKind::Identical { size } => {
                info!("MemoryKind::Identical: size={:#x} ({}MB)", size, size / (1024 * 1024));
                let array = Array::new(*size, ALIGN);

                let hva = HostVirtAddr::from(array.as_mut_ptr() as usize);
                _hpa = virt_to_phys(hva);
                _gpa = GuestPhysAddr::from_usize(_hpa.as_usize()); // GPA == HPA
                _size = *size;
                _payload = Some(array);
                info!("  Allocated: HVA={:#x}, HPA={:#x}, GPA={:#x}", hva, _hpa, _gpa);
                let mut g = self.aspace.lock();
                g.map_linear(
                    _gpa.as_usize().into(),
                    _hpa.as_usize().into(),
                    _size.align_up_4k(),
                    flags,
                )
                .unwrap();
            }
            MemoryKind::Reserved { hpa, size } => {
                info!("MemoryKind::Reserved: HPA={:#x}, size={:#x} ({}MB)", hpa, size, size / (1024 * 1024));
                _gpa = GuestPhysAddr::from_usize(hpa.as_usize()); // GPA == HPA
                _hpa = *hpa;
                _size = *size;
                _payload = None;
                info!("  Mapping: GPA={:#x} -> HPA={:#x}", _gpa, _hpa);
                let mut g = self.aspace.lock();
                g.map_linear(
                    _gpa.as_usize().into(),
                    hpa.as_usize().into(),
                    _size.align_up_4k(),
                    flags,
                )
                .unwrap();
            }
            MemoryKind::Vmem { gpa, size } => {
                info!("MemoryKind::Vmem: GPA={:#x}, size={:#x} ({}MB)", gpa, size, size / (1024 * 1024));
                _gpa = *gpa;
                _size = *size;
                _payload = None;
                let mut g = self.aspace.lock();
                g.map_alloc(_gpa.as_usize().into(), _size.align_up_4k(), flags, true)
                    .unwrap();
                // Query the allocated HPA for the first page
                _hpa = g
                    .translate(_gpa.as_usize().into())
                    .ok_or(anyhow!("Failed to get HPA for Vmem mapping"))?
                    .as_usize()
                    .into();
            }
        }

        self.memories.push(GuestMemory {
            gpa: _gpa,
            hpa: _hpa,
            layout: Layout::from_size_align(_size, _align).unwrap(),
            _payload,
            aspace: self.aspace.clone(),
        });

        self.region_map.add(VmRegion {
            gpa: _gpa,
            size: _size,
            kind: VmRegionKind::Memory,
        })?;

        Ok(())
    }

    pub fn load_kernel_image(&mut self, config: &AxVMConfig) -> anyhow::Result<()> {
        let mut idx = 0;
        let image_cfg = config.image_config();
        let gpa = if let Some(gpa) = image_cfg.kernel.gpa {
            info!("[load_kernel_image] Using specified kernel GPA: {:#x}", gpa);
            let mut found = false;
            for (i, region) in self.memories.iter().enumerate() {
                info!("[load_kernel_image]   Checking region {}: GPA={:#x}, size={:#x}",
                      i, region.gpa, region.size());
                if (region.gpa..region.gpa + region.size()).contains(&gpa) {
                    idx = i;
                    found = true;
                    info!("[load_kernel_image]   Found in region {}", i);
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
            for (i, region) in self.memories.iter().enumerate() {
                if region.size() >= image_cfg.kernel.data.len() {
                    gpa = Some(region.gpa + 2 * 1024 * 1024);
                    idx = i;
                    break;
                } else {
                    continue;
                }
            }
            gpa.ok_or(anyhow!("No suitable memory region found for kernel image"))?
        };

        info!(
            "[load_kernel_image] Final: GPA={:#x}, memory_region_idx={}, offset_in_region={:#x}",
            gpa.as_usize(),
            idx,
            gpa.as_usize() - self.memories[idx].gpa().as_usize()
        );
        let offset = gpa.as_usize() - self.memories[idx].gpa().as_usize();
        self.memories[idx].copy_from_slice(offset, &image_cfg.kernel.data);
        self.kernel_memory_index = idx;
        self.kernel_entry = gpa;
        Ok(())
    }

    pub fn memories(&self) -> &[GuestMemory] {
        &self.memories
    }

    pub fn load_dtb(&mut self, data: &[u8]) -> anyhow::Result<GuestPhysAddr> {
        let guest_mem = self.memories().iter().next().unwrap();
        let mut dtb_start =
            (guest_mem.gpa().as_usize() + guest_mem.size().min(512 * 1024 * 1024)) - data.len();
        dtb_start = dtb_start.align_down_4k();

        let gpa = GuestPhysAddr::from(dtb_start);
        info!("[load_dtb] DTB will be loaded at GPA={:#x}", dtb_start);
        self.copy_to_guest(gpa, &data);
        Ok(gpa)
    }

    pub fn map_passthrough_regions(&self) -> anyhow::Result<()> {
        // For RISC-V, passthrough addresses are configured in the VM config
        // and should be handled by the config system. This function is kept
        // for compatibility with ARM/x86 architectures.
        debug!("map_passthrough_regions: no-op for RISC-V (handled by config)");
        Ok(())
    }

    /// Add an identity mapping (GPA == HPA) for passthrough device regions.
    ///
    /// This is used for MMIO devices that are passed through to the guest,
    /// such as the UART. The guest can access these devices directly without
    /// hypervisor intervention.
    ///
    /// # Arguments
    ///
    /// * `gpa` - Guest Physical Address (also used as Host Physical Address for identity mapping)
    /// * `size` - Size of the region in bytes
    pub fn add_passthrough_mapping(&mut self, gpa: GuestPhysAddr, size: usize) -> anyhow::Result<()> {
        // For passthrough devices, use DEVICE flags (non-cacheable MMIO)
        let flags = MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE | MappingFlags::USER;

        // Identity mapping: GPA == HPA
        let hpa = gpa.as_usize();

        info!(
            "Adding passthrough mapping: GPA={:#x} -> HPA={:#x}, size={:#x}",
            gpa.as_usize(), hpa, size
        );

        let mut g = self.aspace.lock();
        g.map_linear(
            gpa.as_usize().into(),
            hpa.into(),
            size.align_up_4k(),
            flags,
        ).map_err(|e| anyhow!("Failed to add passthrough mapping at {:#x}: {:?}", gpa.as_usize(), e))?;

        Ok(())
    }

    fn copy_to_guest(&mut self, gpa: GuestPhysAddr, data: &[u8]) {
        let parts = self
            .aspace
            .lock()
            .translated_byte_buffer(gpa.as_usize().into(), data.len())
            .unwrap();
        let mut offset = 0;
        for part in parts {
            let len = part.len().min(data.len() - offset);
            part.copy_from_slice(&data[offset..offset + len]);
            offset += len;
        }
    }

    /// Inject a virtual external interrupt to the guest via the vPLIC.
    ///
    /// On RISC-V, this writes to the vPLIC pending register, which:
    /// 1. Sets the IRQ as pending in the vPLIC's pending bitmap
    /// 2. Sets HVIP.VSEIP to signal the guest that an external interrupt is pending
    ///
    /// The guest will then:
    /// 1. See the VSEIP bit and trap to its external interrupt handler
    /// 2. Read the PLIC claim register (via MMIO trap) to get the IRQ number
    /// 3. Handle the interrupt
    /// 4. Write to the PLIC complete register to acknowledge
    ///
    /// # Arguments
    ///
    /// * `irq` - The PLIC source interrupt number to inject (1-based, 0 is reserved)
    #[cfg(target_arch = "riscv64")]
    pub fn inject_virtual_interrupt(&self, irq: u32) {
        trace!("[VmAddrSpace] inject_virtual_interrupt: irq={}", irq);

        // PLIC pending register layout (per PLIC 1.0.0 spec):
        //   Base + 0x1000 + (irq/32)*4: pending bits word
        //   Writing sets the corresponding source as pending.
        //
        // The vPLIC handler (devops_impl.rs) also calls hvip::set_vseip()
        // when any pending bit is set, which signals the guest.
        const PLIC_BASE: usize = 0x0c00_0000;
        const PLIC_PENDING_OFFSET: usize = 0x1000;

        let word_index = irq as usize / 32;
        let bit_index = irq % 32;
        let pending_reg_addr = PLIC_BASE + PLIC_PENDING_OFFSET + word_index * 4;
        let pending_val = 1usize << bit_index;

        trace!("[VmAddrSpace] Writing to vPLIC pending: addr={:#x}, val={:#x}",
            pending_reg_addr, pending_val);

        if let Err(e) = self.handle_mmio_write(
            GuestPhysAddr::from(pending_reg_addr),
            axaddrspace::device::AccessWidth::Dword,
            pending_val,
        ) {
            error!("Failed to inject virtual interrupt {}: {:?}", irq, e);
        }
    }

    /// Handle MMIO read operation
    pub fn handle_mmio_read(
        &self,
        addr: GuestPhysAddr,
        width: axaddrspace::device::AccessWidth,
    ) -> axerrno::AxResult<usize> {
        use axaddrspace::GuestPhysAddr as AxAddrSpaceGpa;
        let addr = AxAddrSpaceGpa::from(addr.as_usize());
        self.devices.handle_mmio_read(addr, width)
    }

    /// Handle MMIO write operation
    pub fn handle_mmio_write(
        &self,
        addr: GuestPhysAddr,
        width: axaddrspace::device::AccessWidth,
        val: usize,
    ) -> axerrno::AxResult {
        use axaddrspace::GuestPhysAddr as AxAddrSpaceGpa;
        let addr = AxAddrSpaceGpa::from(addr.as_usize());
        self.devices.handle_mmio_write(addr, width, val)
    }

    /// Handle system register read (ARM64 only)
    #[cfg(target_arch = "aarch64")]
    pub fn handle_sys_reg_read(
        &self,
        addr: axaddrspace::device::SysRegAddr,
        width: axaddrspace::device::AccessWidth,
    ) -> axerrno::AxResult<usize> {
        self.devices.handle_sys_reg_read(addr, width)
    }

    /// Handle system register write (ARM64 only)
    #[cfg(target_arch = "aarch64")]
    pub fn handle_sys_reg_write(
        &self,
        addr: axaddrspace::device::SysRegAddr,
        width: axaddrspace::device::AccessWidth,
        val: usize,
    ) -> axerrno::AxResult {
        self.devices.handle_sys_reg_write(addr, width, val)
    }

    /// Handle port read (x86 only)
    #[cfg(target_arch = "x86_64")]
    pub fn handle_port_read(
        &self,
        port: axaddrspace::device::Port,
        width: axaddrspace::device::AccessWidth,
    ) -> axerrno::AxResult<usize> {
        self.devices.handle_port_read(port, width)
    }

    /// Handle port write (x86 only)
    #[cfg(target_arch = "x86_64")]
    pub fn handle_port_write(
        &self,
        port: axaddrspace::device::Port,
        width: axaddrspace::device::AccessWidth,
        val: usize,
    ) -> axerrno::AxResult {
        self.devices.handle_port_write(port, width, val)
    }

    /// Pop pending interrupt for a vCPU (before VM entry)
    pub fn pop_pending_interrupt(&self, cpu_id: usize) -> Option<axdevice::PendingInterrupt> {
        let pending = self.devices.pop_pending_interrupt(cpu_id);
        if pending.is_some() {
            // log::info!("[VmAddrSpace] pop_pending_interrupt: cpu_id={}, found interrupt", cpu_id);
        }
        pending
    }

    /// Inject a passthrough device interrupt into the interrupt queue.
    ///
    /// This method is used when handling external interrupts from passthrough
    /// devices (e.g., physical UART). The interrupt is queued and will be
    /// injected to the guest via vPLIC when the vCPU runs next.
    ///
    /// # Arguments
    ///
    /// * `irq` - The IRQ number from the physical device.
    /// * `cpu_id` - The target vCPU ID.
    pub fn inject_passthrough_interrupt(&self, irq: u32, cpu_id: usize) -> axerrno::AxResult {
        // log::info!(
        //     "[VmAddrSpace] inject_passthrough_interrupt: irq={}, cpu_id={}",
        //     irq,
        //     cpu_id
        // );
        self.devices.inject_passthrough_interrupt(irq, cpu_id)
    }

    /// Add a VirtIO block device to the VM.
    ///
    /// This creates a VirtIO block device with the given backend and adds it
    /// to the device list. The device will use the VM's address space for
    /// guest memory access (DMA).
    ///
    /// # Arguments
    ///
    /// * `base_gpa` - Base guest physical address of the device's MMIO region
    /// * `length` - Size of the MMIO region in bytes
    /// * `irq_id` - Interrupt request ID for this device
    /// * `backend` - Block backend for storage operations
    /// * `capacity_sectors` - Capacity of the device in 512-byte sectors
    ///
    /// # Returns
    ///
    /// The device ID on success, or an error if device creation fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let device_id = vm_addr_space.add_virtio_blk_device(
    ///     GuestPhysAddr::from(0x1000_1000),
    ///     0x1000,
    ///     1,
    ///     my_backend,
    ///     1024 * 1024, // 512MB
    /// )?;
    /// ```
    #[cfg(feature = "virtio-blk")]
    pub fn add_virtio_blk_device<B: axdevice::virtio::BlockBackend + 'static>(
        &mut self,
        base_gpa: GuestPhysAddr,
        length: usize,
        irq_id: u32,
        backend: B,
        capacity_sectors: u64,
    ) -> axerrno::AxResult<axdevice::DeviceId> {
        use alloc::sync::Arc;
        use axaddrspace::GuestPhysAddr as AxAddrSpaceGpa;

        let accessor = self.guest_memory_accessor();
        let base_ipa = AxAddrSpaceGpa::from(base_gpa.as_usize());

        let device = axdevice::virtio::VirtioBlkDeviceBuilder::new()
            .base_address(base_ipa)
            .size(length)
            .irq(irq_id)
            .capacity_sectors(capacity_sectors)
            .build(backend, accessor)?;

        self.devices.try_add_mmio_dev(Arc::new(device))
    }

    /// Get mutable access to the device manager.
    ///
    /// This allows advanced device management operations.
    pub fn devices_mut(&mut self) -> &mut AxVmDevices {
        &mut self.devices
    }

    /// Get reference to the device manager.
    pub fn devices(&self) -> &AxVmDevices {
        &self.devices
    }
}

#[derive(Debug, Clone)]
pub struct VmRegion {
    pub gpa: GuestPhysAddr,
    pub size: usize,
    pub kind: VmRegionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRegionKind {
    Passthrough,
    Memory,
}

impl RangeInfo for VmRegion {
    type Kind = VmRegionKind;

    type Type = GuestPhysAddr;

    fn range(&self) -> core::ops::Range<Self::Type> {
        self.gpa..GuestPhysAddr::from_usize(self.gpa.as_usize() + self.size)
    }

    fn kind(&self) -> &Self::Kind {
        &self.kind
    }

    fn overwritable(&self) -> bool {
        matches!(self.kind, VmRegionKind::Passthrough)
    }

    fn clone_with_range(&self, range: core::ops::Range<Self::Type>) -> Self {
        VmRegion {
            gpa: range.start,
            size: range.end.as_usize() - range.start.as_usize(),
            kind: self.kind,
        }
    }
}

pub struct Array {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for Array {}
unsafe impl Sync for Array {}

impl Array {
    pub fn new(size: usize, align: usize) -> Self {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        Array { ptr, layout }
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Deref for Array {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}

impl DerefMut for Array {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for Array {
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

pub struct GuestMemory {
    gpa: GuestPhysAddr,
    hpa: HostPhysAddr,
    layout: Layout,
    aspace: AddrSpaceSync,
    _payload: Option<Array>,
}

impl GuestMemory {
    pub fn copy_from_slice(&mut self, offset: usize, data: &[u8]) {
        assert!(data.len() <= self.size() - offset);

        let g = self.aspace.lock();
        let hva = g
            .translated_byte_buffer(self.gpa.as_usize().into(), self.size())
            .expect("Failed to translate kernel image load address");
        let mut remain = data;
        let mut skip = offset;

        for buff in hva {
            if skip >= buff.len() {
                skip -= buff.len();
                continue;
            }
            let buff = &mut buff[skip..];
            skip = 0;

            let copy_size = core::cmp::min(remain.len(), buff.len());
            buff[..copy_size].copy_from_slice(&remain[..copy_size]);
            crate::arch::Hal::cache_flush(HostVirtAddr::from(buff.as_ptr() as usize), copy_size);
            remain = &remain[copy_size..];
            if remain.is_empty() {
                break;
            }
        }
    }

    pub fn gpa(&self) -> GuestPhysAddr {
        self.gpa
    }

    pub fn size(&self) -> usize {
        self.layout.size()
    }
}

impl Drop for GuestMemory {
    fn drop(&mut self) {
        let start = self.gpa.as_usize().align_down(self.layout.align());
        let size = self.size().align_up(self.layout.align());

        let mut g = self.aspace.lock();
        g.unmap(start.into(), size).unwrap();
    }
}

/// A GuestMemoryAccessor implementation that wraps the VM's address space.
///
/// This allows VirtIO devices to access guest memory for DMA operations.
/// It translates guest physical addresses to host physical/virtual addresses
/// using the VM's address space page table.
#[derive(Clone)]
pub struct VmGuestMemoryAccessor {
    aspace: AddrSpaceSync,
}

impl VmGuestMemoryAccessor {
    /// Create a new VmGuestMemoryAccessor from the VM's address space.
    pub fn new(aspace: AddrSpaceSync) -> Self {
        Self { aspace }
    }
}

// Safety: VmGuestMemoryAccessor is safe to send between threads because
// AddrSpaceSync (Arc<Mutex<...>>) is Send + Sync.
unsafe impl Send for VmGuestMemoryAccessor {}
unsafe impl Sync for VmGuestMemoryAccessor {}

impl GuestMemoryAccessor for VmGuestMemoryAccessor {
    fn translate_and_get_limit(
        &self,
        guest_addr: axaddrspace::GuestPhysAddr,
    ) -> Option<(PhysAddr, usize)> {
        let g = self.aspace.lock();

        // Translate the guest physical address using the VM's page table
        let host_phys = g.translate(guest_addr.as_usize().into())?;

        // Convert host physical address to host virtual address
        // The trait expects a dereferenceable address despite the PhysAddr return type
        let host_virt = phys_to_virt(host_phys);

        // For simplicity, we return a fixed limit. In a more sophisticated
        // implementation, we would calculate the remaining bytes until the
        // next page boundary or unmapped region.
        // Using 4KB page size as a safe limit.
        let page_offset = guest_addr.as_usize() & 0xFFF;
        let limit = 0x1000 - page_offset;

        Some((PhysAddr::from_usize(host_virt.as_usize()), limit))
    }
}

impl VmAddrSpace {
    /// Get a guest memory accessor for VirtIO device DMA operations.
    ///
    /// This returns a clone-able accessor that can be used by VirtIO devices
    /// to read from and write to guest memory.
    pub fn guest_memory_accessor(&self) -> VmGuestMemoryAccessor {
        VmGuestMemoryAccessor::new(self.aspace.clone())
    }

    /// Initialize VirtIO block device with host block device.
    ///
    /// This method should be called after the address space is created to set up
    /// the VirtIO block device with the host block device as backing storage.
    #[cfg(feature = "virtio-blk")]
    pub fn init_virtio_blk<D: axdriver_block::BlockDriverOps + Send + Sync + 'static>(
        &mut self,
        host_device: D,
        base_ipa: axaddrspace::GuestPhysAddr,
        length: usize,
        irq_id: u32,
    ) -> anyhow::Result<()> {
        use axdevice::virtio::{VirtioBlkDevice, VirtioBlkDeviceBuilder};
        use axdevice::AxBlockBackend;
        use axvirtio_blk::VirtioBlockConfig;

        // Create block backend from host device
        let backend = AxBlockBackend::new(host_device);
        let capacity = backend.num_blocks();

        // Get guest memory accessor
        let accessor = self.guest_memory_accessor();

        // Create VirtIO block device
        let device = VirtioBlkDeviceBuilder::new()
            .base_address(base_ipa)
            .size(length)
            .irq(irq_id)
            .capacity_sectors(capacity)
            .build(backend, accessor)
            .map_err(|e| anyhow::anyhow!("Failed to create VirtIO block device: {:?}", e))?;

        // Add to device list with interrupt support
        self.devices.try_add_mmio_dev(alloc::sync::Arc::new(device))
            .map_err(|e| anyhow::anyhow!("Failed to add VirtIO block device: {:?}", e))?;

        info!(
            "VirtIO block device initialized: base_ipa={:#x}, length={:#x}, irq={}, capacity={} sectors",
            base_ipa.as_usize(), length, irq_id, capacity
        );

        Ok(())
    }

    /// Initialize VirtIO console device with platform console backend.
    ///
    /// This method creates a VirtIO console device that uses the host's
    /// UART as backing I/O for the virtual console.
    #[cfg(feature = "virtio-console")]
    pub fn init_virtio_console(
        &mut self,
        base_ipa: axaddrspace::GuestPhysAddr,
        length: usize,
        irq_id: u32,
    ) -> anyhow::Result<()> {
        use axdevice::virtio::{VirtioConsoleDevice, VirtioConsoleDeviceBuilder};
        use axdevice::AxConsoleBackend;

        // Create console backend using platform UART
        let backend = AxConsoleBackend::new();

        // Get guest memory accessor
        let accessor = self.guest_memory_accessor();

        // Create VirtIO console device
        let device = VirtioConsoleDeviceBuilder::new()
            .base_address(base_ipa)
            .size(length)
            .irq(irq_id)
            .terminal_size(80, 25)
            .build(backend, accessor)
            .map_err(|e| anyhow::anyhow!("Failed to create VirtIO console device: {:?}", e))?;

        // Store a reference for console input handling before wrapping
        let device = alloc::sync::Arc::new(device);
        // Register global console handler for vCPU loop
        axdevice::register_console_handler(device.clone() as alloc::sync::Arc<dyn ConsoleInputHandler>);

        // Add to device list with interrupt support
        self.devices.try_add_mmio_dev(device)
            .map_err(|e| anyhow::anyhow!("Failed to add VirtIO console device: {:?}", e))?;

        info!(
            "VirtIO console device initialized: base_ipa={:#x}, length={:#x}, irq={}",
            base_ipa.as_usize(), length, irq_id
        );

        Ok(())
    }
}
