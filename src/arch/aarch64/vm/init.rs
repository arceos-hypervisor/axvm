use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    string::String,
    sync::Arc,
    vec::Vec,
};

use arm_vcpu::Aarch64VCpuSetupConfig;

use crate::{
    GuestPhysAddr, RunError, TASK_STACK_SIZE, VmData, VmStatusInitOps, VmStatusRunningOps,
    VmStatusStoppingOps,
    arch::{VmStatusRunning, cpu::VCpu},
    config::{AxVMConfig, MemoryKind},
    vm::{MappingFlags, VmId},
};

const VM_ASPACE_BASE: GuestPhysAddr = GuestPhysAddr::from_usize(0);
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;
const VM_ASPACE_END: GuestPhysAddr =
    GuestPhysAddr::from_usize(VM_ASPACE_BASE.as_usize() + VM_ASPACE_SIZE);

pub struct VmInit {
    pub id: VmId,
    pub name: String,
    pt_levels: usize,
    stop_requested: AtomicBool,
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
            run_data: None,
        };
        Ok(vm)
    }

    /// Initializes the VM, creating vCPUs and setting up memory
    pub fn init(&mut self, config: AxVMConfig) -> anyhow::Result<()> {
        debug!("Initializing VM {} ({})", self.id, self.name);

        let vcpus = self.new_vcpus(&config)?;

        let mut run_data = VmStatusRunning::new(
            VmData::new(self.pt_levels, VM_ASPACE_BASE..VM_ASPACE_END)?,
            vcpus,
        );

        debug!("Mapping memory regions for VM {} ({})", self.id, self.name);
        for memory_cfg in &config.memory_regions {
            use crate::vm::MappingFlags;
            let m = run_data.data.new_memory(
                memory_cfg,
                MappingFlags::READ
                    | MappingFlags::WRITE
                    | MappingFlags::EXECUTE
                    | MappingFlags::USER,
            );
            run_data.data.add_memory(m);
        }

        run_data.data.load_kernel_image(&config)?;
        run_data.make_dtb(&config)?;

        run_data.data.map_passthrough_regions()?;

        let kernel_entry = run_data.data.kernel_entry();
        let gpt_root = run_data.data.gpt_root();

        // Setup vCPUs
        for vcpu in &mut run_data.vcpus {
            vcpu.vcpu.set_entry(kernel_entry).unwrap();
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
                .set_ept_root(gpt_root)
                .map_err(|e| anyhow::anyhow!("Failed to set EPT root for vCPU : {e:?}"))?;

            run_data.vcpu_running_count.fetch_add(1, Ordering::SeqCst);
        }

        self.run_data = Some(run_data);

        Ok(())
    }

    fn new_vcpus(&mut self, config: &AxVMConfig) -> anyhow::Result<Vec<VCpu>> {
        // Create vCPUs
        let mut vcpus = Vec::new();

        let dtb_addr = GuestPhysAddr::from_usize(0);

        match config.cpu_num {
            crate::config::CpuNumType::Alloc(num) => {
                for _ in 0..num {
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
        Ok(vcpus)
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
