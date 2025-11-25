use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::AddrSpace;
use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{
    GuestPhysAddr,
    arch::{RunData, cpu::VCpu},
    config::AxVMConfig,
    vhal::cpu::CpuId,
    vm::{Status, VmId, VmOps},
};

const VM_ASPACE_BASE: usize = 0x0;
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;

/// AArch64 Virtual Machine implementation
pub struct ArchVm {
    pub id: VmId,
    pub name: String,
    pt_levels: usize,
    state: Option<StateMachine>,
    stop_requested: AtomicBool,
    exit_code: AtomicUsize,
}

impl ArchVm {
    /// Creates a new VM with the given configuration
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let vm = Self {
            id: config.id().into(),
            name: config.name(),
            pt_levels: 4,
            state: Some(StateMachine::Idle(config)),
            stop_requested: AtomicBool::new(false),
            exit_code: AtomicUsize::new(0),
        };
        Ok(vm)
    }

    /// Initializes the VM, creating vCPUs and setting up memory
    pub fn init(&mut self) -> anyhow::Result<()> {
        debug!("Initializing VM {} ({})", self.id, self.name);
        let StateMachine::Idle(config) = self.state.take().unwrap() else {
            return Err(anyhow::anyhow!("VM is not in Idle state"));
        };

        // Create vCPUs
        let mut vcpus = Vec::new();
        // let dtb_addr = config
        //     .image_config
        //     .dtb_load_gpa
        //     .map(|d| d.as_usize())
        //     .unwrap_or_default();

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
                // let new_data = RunData {
                //     // vcpus: BTreeMap::new(),
                //     // address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
                //     devices: BTreeMap::new(),
                // };
                // let old_data = core::mem::replace(data, new_data);
                // self.transition_state(StateMachine::ShuttingDown(old_data))?;

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
                // data.vcpus.clear();

                // Note: We don't destroy the address space here as it might be needed
                // for debugging or inspection after shutdown

                Ok(())
            }
            _ => Err(anyhow::anyhow!("VM is not in ShuttingDown state")),
        }
    }
}

impl VmOps for ArchVm {
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
        // let new_data = RunData {
        //     // vcpus: BTreeMap::new(),
        //     // address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
        //     devices: BTreeMap::new(),
        // };
        // let old_data = core::mem::replace(data, new_data);
        // self.transition_state(StateMachine::Running(old_data))?;

        // // Start all vCPUs
        // let vcpus = self.get_vcpus();
        // for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
        //     debug!("Starting vCPU {} for VM {}", vcpu_id, self.id);
        //     vcpu.bind()
        //         .map_err(|e| anyhow::anyhow!("Failed to bind vCPU {}: {:?}", vcpu_id, e))?;
        // }

        // info!(
        //     "VM {} ({}) booted successfully with {} vCPUs",
        //     self.id,
        //     self.name,
        //     vcpus.len()
        // );

        Ok(())
    }

    fn stop(&self) {
        if !self.is_active() {
            return; // Already stopped
        }

        info!("Stopping VM {} ({})", self.id, self.name);

        // Set stop flag
        self.stop_requested.store(true, Ordering::SeqCst);

        // // Unbind all vCPUs
        // let vcpus = self.get_vcpus();
        // for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
        //     debug!("Unbinding vCPU {} for VM {}", vcpu_id, self.id);
        //     if let Err(e) = vcpu.unbind() {
        //         warn!("Failed to unbind vCPU {}: {:?}", vcpu_id, e);
        //     }
        // }

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

impl Drop for ArchVm {
    fn drop(&mut self) {
        // Ensure VM is properly shut down
        if matches!(self.get_state(), StateMachine::Running(_)) {
            let _ = self.shutdown();
        }
    }
}

impl ArchVm {
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

        // // Transition to Inited state
        // let new_data = RunData {
        //     // vcpus: BTreeMap::new(),
        //     // address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
        //     devices: BTreeMap::new(),
        // };
        // let old_data = core::mem::replace(data, new_data);
        // self.transition_state(StateMachine::Inited(old_data))?;

        // // Unbind all vCPUs
        // let vcpus = self.get_vcpus();
        // for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
        //     debug!("Unbinding vCPU {} for VM {}", vcpu_id, self.id);
        //     if let Err(e) = vcpu.unbind() {
        //         warn!("Failed to unbind vCPU {}: {:?}", vcpu_id, e);
        //     }
        // }

        info!("VM {} ({}) paused", self.id, self.name);
        Ok(())
    }

    /// Resumes the VM
    pub fn resume(&mut self) -> anyhow::Result<()> {
        let data = match self.get_state_mut() {
            StateMachine::Inited(data) => data,
            _ => return Err(anyhow::anyhow!("VM is not in Inited state")),
        };

        // // Transition to Running state
        // let new_data = RunData {
        //     // vcpus: BTreeMap::new(),
        //     // address_space: AddrSpace::new_empty(4, GuestPhysAddr::from(0), 0).unwrap(),
        //     devices: BTreeMap::new(),
        // };
        // let old_data = core::mem::replace(data, new_data);
        // self.transition_state(StateMachine::Running(old_data))?;

        // // Bind all vCPUs
        // let vcpus = self.get_vcpus();
        // for (vcpu_id, vcpu) in vcpus.iter().enumerate() {
        //     debug!("Binding vCPU {} for VM {}", vcpu_id, self.id);
        //     if let Err(e) = vcpu.bind() {
        //         warn!("Failed to bind vCPU {}: {:?}", vcpu_id, e);
        //     }
        // }

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
        // info!("  State: {}", self.state_str());
        // info!("  vCPUs: {}", self.vcpu_count());
        // info!("  Devices: {}", self.get_devices().len());

        // if let Some(root) = self.page_table_root() {
        // info!("  Page Table Root: {:#x}", root);
        // }
    }
}

/// VM state machine
enum StateMachine {
    Idle(AxVMConfig),
    Inited(RunData),
    Running(RunData),
    ShuttingDown(RunData),
    PoweredOff,
}
