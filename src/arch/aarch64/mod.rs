use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use memory_addr::VirtAddr;

use crate::alloc::alloc::{self, Layout};
use crate::alloc::collections::BTreeMap;
use crate::alloc::string::String;
use crate::alloc::sync::Arc;
use crate::alloc::vec;
use crate::alloc::vec::Vec;

use axaddrspace::{AddrSpace, AxMmHal, GuestPhysAddr, HostPhysAddr, MappingFlags};
use axerrno::{AxResult, ax_err};
use axvcpu::{AxArchVCpu, AxVCpu, AxVCpuHal};
use page_table_multiarch::PagingHandler;

use crate::vcpu::{AxArchVCpuImpl, AxVCpuCreateConfig, AxVCpuSetupConfig};
use crate::{config::AxVMConfig, vm2::*};

/// A virtual CPU with architecture-independent interface.
type VCpu<U> = AxVCpu<AxArchVCpuImpl<U>>;
/// A reference to a vCPU.
pub type AxVCpuRef<U> = Arc<VCpu<U>>;

// Implement Display for VmId
impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VmId({:?})", self)
    }
}

// 临时占位符实现，实际使用时需要替换为正确的实现
// 使用newtype模式来避免orphan rule
struct DummyHal;
impl AxVCpuHal for DummyHal {
    type MmHal = DummyPagingHandler;
}

struct DummyPagingHandler;
impl AxMmHal for DummyPagingHandler {
    fn alloc_frame() -> Option<HostPhysAddr> {
        todo!("alloc_frame")
    }

    fn dealloc_frame(_paddr: HostPhysAddr) {
        todo!("dealloc_frame")
    }

    fn phys_to_virt(_paddr: HostPhysAddr) -> VirtAddr {
        // 临时实现，返回一个虚拟地址
        // 实际实现需要根据具体的内存映射方案
        VirtAddr::from(0x40000000usize)
    }

    fn virt_to_phys(_vaddr: VirtAddr) -> HostPhysAddr {
        todo!("virt_to_phys")
    }
}

impl PagingHandler for DummyPagingHandler {
    fn alloc_frame() -> Option<HostPhysAddr> {
        todo!("alloc_frame")
    }

    fn dealloc_frame(_paddr: HostPhysAddr) {
        todo!("dealloc_frame")
    }

    fn phys_to_virt(_paddr: HostPhysAddr) -> VirtAddr {
        // 临时实现，返回一个虚拟地址
        // 实际实现需要根据具体的内存映射方案
        VirtAddr::from(0x40000000usize)
    }
}

/// Data needed when VM is running
pub struct RunData {
    vcpus: BTreeMap<usize, AxVCpuRef<DummyHal>>,
    address_space: AddrSpace<DummyPagingHandler>,
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

/// VM state machine
enum StateMachine {
    Idle(AxVMConfig),
    Inited(RunData),
    Running(RunData),
    ShuttingDown(RunData),
    PoweredOff,
}

/// AArch64 Virtual Machine implementation
pub struct Vm {
    id: VmId,
    name: String,
    state: Option<StateMachine>,
    stop_requested: AtomicBool,
    exit_code: AtomicUsize,
}

impl Vm {
    /// Creates a new VM with the given configuration
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let vm = Self {
            id: config.id().into(),
            name: config.name(),
            state: Some(StateMachine::Idle(config)),
            stop_requested: AtomicBool::new(false),
            exit_code: AtomicUsize::new(0),
        };
        Ok(vm)
    }

    /// Initializes the VM, creating vCPUs and setting up memory
    pub fn init(&mut self) -> anyhow::Result<()> {
        let StateMachine::Idle(config) = self.state.take().unwrap() else {
            return Err(anyhow::anyhow!("VM is not in Idle state"));
        };

        // // Create address space for the VM
        // let address_space = AddrSpace::new_empty(GuestPhysAddr::from(0x0), 0x7fff_ffff_f000)
        //     .map_err(|e| anyhow::anyhow!("Failed to create address space: {:?}", e))?;

        // // Create vCPUs
        // let mut vcpus = BTreeMap::new();
        // let vcpu_count = config.phys_cpu_ls.cpu_num();

        // for vcpu_id in 0..vcpu_count {
        //     let dtb_addr = config
        //         .image_config()
        //         .dtb_load_gpa
        //         .unwrap_or_default()
        //         .as_usize();

        //     let arch_config = AxVCpuCreateConfig {
        //         mpidr_el1: vcpu_id as u64,
        //         dtb_addr,
        //     };

        //     let vcpu: AxArchVCpu<DummyHal> = AxArchVCpu::new(config.id(), vcpu_id, arch_config)
        //         .map_err(|e| anyhow::anyhow!("Failed to create vCPU {}: {:?}", vcpu_id, e))?;

        //     vcpus.insert(vcpu_id, Arc::new(vcpu));
        // }

        // // Initialize devices
        // let mut devices = BTreeMap::new();

        // // Add emulated devices
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

        // // Setup vCPUs
        // for (vcpu_id, vcpu) in &vcpus {
        //     let entry = if *vcpu_id == 0 {
        //         config.bsp_entry()
        //     } else {
        //         config.ap_entry()
        //     };

        //     let setup_config = AxVCpuSetupConfig {
        //         passthrough_interrupt: config.interrupt_mode()
        //             == axvmconfig::VMInterruptMode::Passthrough,
        //         passthrough_timer: config.interrupt_mode()
        //             == axvmconfig::VMInterruptMode::Passthrough,
        //     };

        //     // Set entry point first
        //     vcpu.set_entry(entry).map_err(|e| {
        //         anyhow::anyhow!("Failed to set entry for vCPU {}: {:?}", vcpu_id, e)
        //     })?;

        //     // Set EPT root
        //     vcpu.set_ept_root(address_space.page_table_root())
        //         .map_err(|e| {
        //             anyhow::anyhow!("Failed to set EPT root for vCPU {}: {:?}", vcpu_id, e)
        //         })?;

        //     // Setup vCPU with configuration
        //     vcpu.setup(setup_config)
        //         .map_err(|e| anyhow::anyhow!("Failed to setup vCPU {}: {:?}", vcpu_id, e))?;
        // }

        // self.state = Some(StateMachine::Inited(RunData {
        //     vcpus,
        //     address_space,
        //     devices,
        // }));

        Ok(())
    }

    /// Checks if the VM is active (not stopped)
    fn is_active(&self) -> bool {
        !self.stop_requested.load(Ordering::SeqCst)
    }

    /// Gets the current state of the VM
    fn get_state(&self) -> &StateMachine {
        self.state.as_ref().unwrap()
    }

    /// Gets a mutable reference to the current state of the VM
    fn get_state_mut(&mut self) -> &mut StateMachine {
        self.state.as_mut().unwrap()
    }

    /// Transitions the VM state from current to new state
    fn transition_state(&mut self, new_state: StateMachine) -> anyhow::Result<()> {
        let current_state = self.get_state();

        // Validate state transition
        match (current_state, &new_state) {
            (StateMachine::Idle(_), StateMachine::Inited(_)) => {}
            (StateMachine::Inited(_), StateMachine::Running(_)) => {}
            (StateMachine::Running(_), StateMachine::ShuttingDown(_)) => {}
            (StateMachine::ShuttingDown(_), StateMachine::PoweredOff) => {}
            _ => return Err(anyhow::anyhow!("Invalid state transition")),
        }

        self.state = Some(new_state);
        Ok(())
    }

    /// Gets the vCPU with the given ID
    fn get_vcpu(&self, vcpu_id: usize) -> Option<AxVCpuRef<DummyHal>> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data.vcpus.get(&vcpu_id).cloned(),
            _ => None,
        }
    }

    /// Gets all vCPUs of VM
    fn get_vcpus(&self) -> Vec<AxVCpuRef<DummyHal>> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data.vcpus.values().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// Gets address space of VM
    fn get_address_space(&self) -> Option<&AddrSpace<DummyPagingHandler>> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => Some(&data.address_space),
            _ => None,
        }
    }

    /// Maps a memory region in VM
    fn map_region(
        &self,
        gpa: GuestPhysAddr,
        hpa: HostPhysAddr,
        size: usize,
        flags: MappingFlags,
    ) -> AxResult<()> {
        let address_space = match self.get_address_space() {
            Some(aspace) => aspace,
            None => return ax_err!(BadState, "VM is not initialized"),
        };

        debug!(
            "Mapping memory region GPA {:#x} -> HPA {:#x}, size {:#x}, flags {:?}",
            gpa, hpa, size, flags
        );

        // Since we can't modify the address_space directly, we need to use a different approach
        // For now, just return success
        Ok(())
    }

    /// Unmaps a memory region in VM
    fn unmap_region(&self, gpa: GuestPhysAddr, size: usize) -> AxResult<()> {
        let _address_space = match self.get_address_space() {
            Some(aspace) => aspace,
            None => return ax_err!(BadState, "VM is not initialized"),
        };

        debug!("Unmapping memory region GPA {:#x}, size {:#x}", gpa, size);

        // Since we can't modify the address_space directly, we need to use a different approach
        // For now, just return success
        Ok(())
    }

    /// Gets the page table root of the VM
    pub fn page_table_root(&self) -> Option<HostPhysAddr> {
        self.get_address_space()
            .map(|aspace| aspace.page_table_root())
    }

    /// Allocates a memory region for the VM
    pub fn alloc_memory_region(
        &self,
        size: usize,
        gpa: Option<GuestPhysAddr>,
    ) -> anyhow::Result<(GuestPhysAddr, HostPhysAddr)> {
        todo!()
        //     // Allocate memory
        //     let layout = Layout::from_size_align(size, 4096)
        //         .map_err(|_| ax_err!(InvalidInput, "Invalid size or alignment"))?;

        //     let hva = unsafe { alloc::alloc_zeroed(layout) };
        //     if hva.is_null() {
        //         return ax_err!(NoMemory, "Failed to allocate memory");
        //     }

        //     let hva = axaddrspace::HostVirtAddr::from(hva as usize);
        //     // TODO: Replace with actual implementation
        //     let hpa = HostPhysAddr::from(hva.as_usize());

        //     // Use provided GPA or use HPA as GPA
        //     let gpa = gpa.unwrap_or_else(|| GuestPhysAddr::from(hpa.as_usize()));

        //     // Map the memory
        //     self.map_region(
        //         gpa,
        //         hpa,
        //         size,
        //         MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE | MappingFlags::USER,
        //     )?;

        //     debug!(
        //         "Allocated memory region GPA {:#x} -> HPA {:#x}, size {:#x}",
        //         gpa, hpa, size
        //     );

        //     Ok((gpa, hpa))
        // }

        // /// Reads data from guest memory
        // pub fn read_guest_memory(&self, gpa: GuestPhysAddr, buf: &mut [u8]) -> AxResult<()> {
        //     let address_space = match self.get_address_space() {
        //         Some(aspace) => aspace,
        //         None => return ax_err!(BadState, "VM is not initialized"),
        //     };

        //     let buffers = match address_space.translated_byte_buffer(gpa, buf.len()) {
        //         Some(buffers) => buffers,
        //         None => return ax_err!(InvalidInput, "Failed to translate guest address"),
        //     };

        //     let mut offset = 0;
        //     for chunk in buffers {
        //         let copy_len = core::cmp::min(chunk.len(), buf.len() - offset);
        //         buf[offset..offset + copy_len].copy_from_slice(&chunk[..copy_len]);
        //         offset += copy_len;

        //         if offset >= buf.len() {
        //             break;
        //         }
        //     }

        // Ok(())
    }

    /// Writes data to guest memory
    pub fn write_guest_memory(&self, gpa: GuestPhysAddr, data: &[u8]) -> AxResult<()> {
        let address_space = match self.get_address_space() {
            Some(aspace) => aspace,
            None => return ax_err!(BadState, "VM is not initialized"),
        };

        let buffers = match address_space.translated_byte_buffer(gpa, data.len()) {
            Some(buffers) => buffers,
            None => return ax_err!(InvalidInput, "Failed to translate guest address"),
        };

        let mut offset = 0;
        for chunk in buffers {
            let copy_len = core::cmp::min(chunk.len(), data.len() - offset);
            chunk[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);
            offset += copy_len;

            if offset >= data.len() {
                break;
            }
        }

        Ok(())
    }

    /// Reads a value of type T from guest memory
    pub fn read_guest_val<T>(&self, gpa: GuestPhysAddr) -> AxResult<T> {
        // let size = core::mem::size_of::<T>();
        // let mut buf = vec![0u8; size];

        // self.read_guest_memory(gpa, &mut buf)?;

        // // SAFETY: We're reading from a buffer that contains valid data
        // Ok(unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const T) })
        todo!()
    }

    /// Writes a value of type T to guest memory
    pub fn write_guest_val<T>(&self, gpa: GuestPhysAddr, val: &T) -> AxResult<()> {
        let data = unsafe {
            core::slice::from_raw_parts(val as *const T as *const u8, core::mem::size_of::<T>())
        };

        self.write_guest_memory(gpa, data)
    }

    /// Gets information about a device
    pub fn get_device(&self, name: &str) -> Option<DeviceInfo> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data.devices.get(name).cloned(),
            _ => None,
        }
    }

    /// Gets all devices in the VM
    pub fn get_devices(&self) -> Vec<(String, DeviceInfo)> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data
                .devices
                .iter()
                .map(|(name, info)| (name.clone(), info.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Adds a new device to VM
    pub fn add_device(&mut self, name: String, device_info: DeviceInfo) -> AxResult<()> {
        // Map device memory if needed
        if let Some(hpa) = device_info.hpa {
            self.map_region(
                device_info.gpa,
                hpa,
                device_info.size,
                MappingFlags::DEVICE | MappingFlags::READ | MappingFlags::WRITE,
            )?;
        }

        // Now add the device
        let data = match self.get_state_mut() {
            StateMachine::Inited(data) => data,
            _ => return ax_err!(BadState, "VM is not in Inited state"),
        };

        data.devices.insert(name, device_info);
        Ok(())
    }

    /// Removes a device from the VM
    pub fn remove_device(&mut self, name: &str) -> AxResult<()> {
        match self.get_state_mut() {
            StateMachine::Inited(data) => {
                if let Some(device_info) = data.devices.remove(name) {
                    // Unmap device memory
                    self.unmap_region(device_info.gpa, device_info.size)?;
                }
                Ok(())
            }
            _ => ax_err!(BadState, "VM is not in Inited state"),
        }
    }

    /// Handles MMIO read from a device
    pub fn handle_mmio_read(&self, addr: GuestPhysAddr, width: usize) -> AxResult<u64> {
        // Find device that contains this address
        let devices = self.get_devices();
        for (name, device_info) in devices {
            if addr.as_usize() >= device_info.gpa.as_usize()
                && addr.as_usize() < device_info.gpa.as_usize() + device_info.size
            {
                debug!(
                    "MMIO read from device {} at address {:#x}, width {}",
                    name, addr, width
                );

                // For now, return 0 for all reads
                // In a real implementation, this would delegate to the specific device
                return Ok(0);
            }
        }

        ax_err!(InvalidInput, "Address not mapped to any device")
    }

    /// Handles MMIO write to a device
    pub fn handle_mmio_write(&self, addr: GuestPhysAddr, width: usize, data: u64) -> AxResult<()> {
        // Find device that contains this address
        let devices = self.get_devices();
        for (name, device_info) in devices {
            if addr.as_usize() >= device_info.gpa.as_usize()
                && addr.as_usize() < device_info.gpa.as_usize() + device_info.size
            {
                debug!(
                    "MMIO write to device {} at address {:#x}, width {}, data {:#x}",
                    name, addr, width, data
                );

                // For now, just log the write
                // In a real implementation, this would delegate to the specific device
                return Ok(());
            }
        }

        ax_err!(InvalidInput, "Address not mapped to any device")
    }

    /// Runs a specific vCPU
    fn run_vcpu(&self, vcpu_id: usize) -> anyhow::Result<axvcpu::AxVCpuExitReason> {
        // let vcpu = self
        //     .get_vcpu(vcpu_id)
        //     .ok_or_else(|| ax_err!(InvalidInput, "Invalid vCPU ID"))?;

        // if !self.is_active() {
        //     return ax_err!(BadState, "VM is not active");
        // }

        // debug!("Running vCPU {} for VM {}", vcpu_id, self.id);
        // vcpu.bind()?;
        // let exit_reason = vcpu.run()?;
        // vcpu.unbind()?;

        // debug!(
        //     "vCPU {} for VM {} exited with reason: {:?}",
        //     vcpu_id, self.id, exit_reason
        // );
        // Ok(exit_reason)
        todo!()
    }

    /// Injects an interrupt to a vCPU
    fn inject_interrupt(&self, vcpu_id: usize, vector: usize) -> AxResult<()> {
        let vcpu = match self.get_vcpu(vcpu_id) {
            Some(vcpu) => vcpu,
            None => return ax_err!(InvalidInput, "Invalid vCPU ID"),
        };

        debug!(
            "Injecting interrupt {} to vCPU {} for VM {}",
            vector, vcpu_id, self.id
        );
        vcpu.inject_interrupt(vector)
    }

    /// Gets the number of vCPUs in the VM
    pub fn vcpu_count(&self) -> usize {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data.vcpus.len(),
            _ => 0,
        }
    }

    /// Gets the IDs of all vCPUs in the VM
    pub fn vcpu_ids(&self) -> Vec<usize> {
        match self.get_state() {
            StateMachine::Inited(data)
            | StateMachine::Running(data)
            | StateMachine::ShuttingDown(data) => data.vcpus.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// Checks if a vCPU with the given ID exists
    pub fn has_vcpu(&self, vcpu_id: usize) -> bool {
        self.get_vcpu(vcpu_id).is_some()
    }

    /// Sets a general-purpose register of a vCPU
    pub fn set_vcpu_gpr(&self, vcpu_id: usize, reg: usize, val: usize) -> AxResult<()> {
        let vcpu = match self.get_vcpu(vcpu_id) {
            Some(vcpu) => vcpu,
            None => return ax_err!(InvalidInput, "Invalid vCPU ID"),
        };

        vcpu.set_gpr(reg, val);
        Ok(())
    }

    /// Sets the return value of a vCPU
    pub fn set_vcpu_return_value(&self, vcpu_id: usize, val: usize) -> AxResult<()> {
        let vcpu = match self.get_vcpu(vcpu_id) {
            Some(vcpu) => vcpu,
            None => return ax_err!(InvalidInput, "Invalid vCPU ID"),
        };

        vcpu.set_return_value(val);
        Ok(())
    }

    /// Shuts down VM and transitions to PoweredOff state
    pub fn shutdown(&mut self) -> anyhow::Result<()> {
        // First check if we're in Running state
        let is_running = matches!(self.get_state(), StateMachine::Running(_));

        if is_running {
            // Stop VM first
            self.stop();
        }

        match self.get_state_mut() {
            StateMachine::Running(data) => {
                // Transition to ShuttingDown state
                let new_data = RunData {
                    vcpus: BTreeMap::new(),
                    address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
                    devices: BTreeMap::new(),
                };
                let old_data = core::mem::replace(data, new_data);
                self.transition_state(StateMachine::ShuttingDown(old_data))?;

                // Clean up resources
                self.cleanup_resources()?;

                // Transition to PoweredOff state
                self.transition_state(StateMachine::PoweredOff)?;

                info!("VM {} ({}) shut down successfully", self.id, self.name);
                Ok(())
            }
            StateMachine::ShuttingDown(_) => {
                // Already shutting down
                Ok(())
            }
            StateMachine::PoweredOff => {
                // Already powered off
                Ok(())
            }
            _ => Err(anyhow::anyhow!("VM is not in Running state")),
        }
    }

    /// Clean up VM resources
    fn cleanup_resources(&mut self) -> anyhow::Result<()> {
        match self.get_state_mut() {
            StateMachine::ShuttingDown(data) => {
                // Clear vCPUs
                data.vcpus.clear();

                // Note: We don't destroy the address space here as it might be needed
                // for debugging or inspection after shutdown

                Ok(())
            }
            _ => Err(anyhow::anyhow!("VM is not in ShuttingDown state")),
        }
    }
}

impl VmOps for Vm {
    fn id(&self) -> VmId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn boot(&mut self) -> anyhow::Result<()> {
        let data = match self.get_state_mut() {
            StateMachine::Inited(data) => data,
            _ => return Err(anyhow::anyhow!("VM is not in Inited state")),
        };

        // Transition to Running state
        let new_data = RunData {
            vcpus: BTreeMap::new(),
            address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
            devices: BTreeMap::new(),
        };
        let old_data = core::mem::replace(data, new_data);
        self.transition_state(StateMachine::Running(old_data))?;

        // Start all vCPUs
        let vcpus = self.get_vcpus();
        for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
            debug!("Starting vCPU {} for VM {}", vcpu_id, self.id);
            vcpu.bind()
                .map_err(|e| anyhow::anyhow!("Failed to bind vCPU {}: {:?}", vcpu_id, e))?;
        }

        info!(
            "VM {} ({}) booted successfully with {} vCPUs",
            self.id,
            self.name,
            vcpus.len()
        );

        Ok(())
    }

    fn stop(&self) {
        if !self.is_active() {
            return; // Already stopped
        }

        info!("Stopping VM {} ({})", self.id, self.name);

        // Set stop flag
        self.stop_requested.store(true, Ordering::SeqCst);

        // Unbind all vCPUs
        let vcpus = self.get_vcpus();
        for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
            debug!("Unbinding vCPU {} for VM {}", vcpu_id, self.id);
            if let Err(e) = vcpu.unbind() {
                warn!("Failed to unbind vCPU {}: {:?}", vcpu_id, e);
            }
        }

        info!("VM {} ({}) stopped", self.id, self.name);
    }

    fn status(&self) -> Status {
        match self.get_state() {
            StateMachine::Idle(_) => Status::Idle,
            StateMachine::Inited(_) => Status::Idle,
            StateMachine::Running(_) => Status::Running,
            StateMachine::ShuttingDown(_) => Status::ShuttingDown,
            StateMachine::PoweredOff => Status::PoweredOff,
        }
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        // Ensure VM is properly shut down
        if matches!(self.get_state(), StateMachine::Running(_)) {
            let _ = self.shutdown();
        }
    }
}

impl Vm {
    /// Gets the exit code of the VM
    pub fn exit_code(&self) -> usize {
        self.exit_code.load(Ordering::SeqCst)
    }

    /// Sets the exit code of the VM
    pub fn set_exit_code(&self, code: usize) {
        self.exit_code.store(code, Ordering::SeqCst);
    }

    /// Checks if the VM has been stopped
    pub fn is_stopped(&self) -> bool {
        self.stop_requested.load(Ordering::SeqCst)
    }

    /// Resets the VM to initial state
    pub fn reset(&mut self) -> anyhow::Result<()> {
        match self.get_state() {
            StateMachine::Running(_) | StateMachine::ShuttingDown(_) => {
                // Stop the VM first
                self.stop();

                // Transition to PoweredOff state
                self.transition_state(StateMachine::PoweredOff)?;

                // Note: In a real implementation, we would need to:
                // 1. Reset all vCPUs to initial state
                // 2. Reset memory to initial state
                // 3. Reset devices to initial state
                // 4. Transition back to Idle state

                info!("VM {} ({}) reset", self.id, self.name);
                Ok(())
            }
            _ => Err(anyhow::anyhow!("VM is not in a state that can be reset")),
        }
    }

    /// Pauses the VM
    pub fn pause(&mut self) -> anyhow::Result<()> {
        let data = match self.get_state_mut() {
            StateMachine::Running(data) => data,
            _ => return Err(anyhow::anyhow!("VM is not in Running state")),
        };

        // Transition to Inited state
        let new_data = RunData {
            vcpus: BTreeMap::new(),
            address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
            devices: BTreeMap::new(),
        };
        let old_data = core::mem::replace(data, new_data);
        self.transition_state(StateMachine::Inited(old_data))?;

        // Unbind all vCPUs
        let vcpus = self.get_vcpus();
        for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
            debug!("Unbinding vCPU {} for VM {}", vcpu_id, self.id);
            if let Err(e) = vcpu.unbind() {
                warn!("Failed to unbind vCPU {}: {:?}", vcpu_id, e);
            }
        }

        info!("VM {} ({}) paused", self.id, self.name);
        Ok(())
    }

    /// Resumes the VM
    pub fn resume(&mut self) -> anyhow::Result<()> {
        let data = match self.get_state_mut() {
            StateMachine::Inited(data) => data,
            _ => return Err(anyhow::anyhow!("VM is not in Inited state")),
        };

        // Transition to Running state
        let new_data = RunData {
            vcpus: BTreeMap::new(),
            address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
            devices: BTreeMap::new(),
        };
        let old_data = core::mem::replace(data, new_data);
        self.transition_state(StateMachine::Running(old_data))?;

        // Bind all vCPUs
        let vcpus = self.get_vcpus();
        for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
            debug!("Binding vCPU {} for VM {}", vcpu_id, self.id);
            if let Err(e) = vcpu.bind() {
                warn!("Failed to bind vCPU {}: {:?}", vcpu_id, e);
            }
        }

        info!("VM {} ({}) resumed", self.id, self.name);
        Ok(())
    }

    /// Gets the current state as a string
    pub fn state_str(&self) -> &'static str {
        match self.get_state() {
            StateMachine::Idle(_) => "Idle",
            StateMachine::Inited(_) => "Inited",
            StateMachine::Running(_) => "Running",
            StateMachine::ShuttingDown(_) => "ShuttingDown",
            StateMachine::PoweredOff => "PoweredOff",
        }
    }

    /// Prints VM information
    pub fn print_info(&self) {
        info!("VM Information:");
        info!("  ID: {}", self.id);
        info!("  Name: {}", self.name);
        info!("  State: {}", self.state_str());
        info!("  vCPUs: {}", self.vcpu_count());
        info!("  Devices: {}", self.get_devices().len());

        if let Some(root) = self.page_table_root() {
            info!("  Page Table Root: {:#x}", root);
        }
    }
}
