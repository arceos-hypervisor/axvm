use core::sync::atomic::Ordering;

use alloc::vec::Vec;

use crate::{
    AxVMConfig, GuestPhysAddr, VmMachineUninitOps, VmRunCommonData,
    arch::{VmMachineInited, VmStatusRunning, cpu::VCpu},
    config::CpuNumType,
    data2::VmDataWeak,
    vm::MappingFlags,
};

const VM_ASPACE_BASE: GuestPhysAddr = GuestPhysAddr::from_usize(0);
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;
const VM_ASPACE_END: GuestPhysAddr =
    GuestPhysAddr::from_usize(VM_ASPACE_BASE.as_usize() + VM_ASPACE_SIZE);

pub struct VmMachineUninit {
    config: AxVMConfig,
    pt_levels: usize,
}

impl VmMachineUninitOps for VmMachineUninit {
    type Inited = VmMachineInited;

    fn new(config: AxVMConfig) -> Self {
        Self {
            config,
            pt_levels: 4,
        }
    }

    fn init(self, vmdata: VmDataWeak) -> Result<Self::Inited, (anyhow::Error, Self)>
    where
        Self: Sized,
    {
        debug!("Initializing VM {} ({})", self.config.id, self.config.name);
        let cpus = self.new_vcpus(&vmdata)?;
        let mut run_data = VmStatusRunning::new(
            VmRunCommonData::new(self.pt_levels, VM_ASPACE_BASE..VM_ASPACE_END)?,
            vcpus,
        );

        debug!(
            "Mapping memory regions for VM {} ({})",
            self.config.id, self.config.name
        );
        for memory_cfg in &self.config.memory_regions {
            let m = run_data.data.try_use()?.new_memory(
                memory_cfg,
                MappingFlags::READ
                    | MappingFlags::WRITE
                    | MappingFlags::EXECUTE
                    | MappingFlags::USER,
            );
            run_data.data.add_memory(m);
        }

        run_data.data.try_use()?.load_kernel_image(&self.config)?;
        run_data.make_dtb(&self.config)?;

        run_data.data.try_use()?.map_passthrough_regions()?;

        let kernel_entry = run_data.data.try_use()?.kernel_entry();
        let gpt_root = run_data.data.try_use()?.gpt_root();

        // Setup vCPUs
        for vcpu in &mut run_data.vcpus {
            vcpu.vcpu.set_entry(kernel_entry).unwrap();
            vcpu.vcpu.set_dtb_addr(run_data.dtb_addr).unwrap();

            let setup_config = Aarch64VCpuSetupConfig {
                passthrough_interrupt: self.config.interrupt_mode()
                    == axvmconfig::VMInterruptMode::Passthrough,
                passthrough_timer: self.config.interrupt_mode()
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

        Ok(VmMachineInited {
            id: self.config.id,
            name: self.config.name,
            run_data,
        })
    }
}

impl VmMachineUninit {
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

        for vcpu in &vcpus {
            let max_levels = vcpu.with_hcpu(|cpu| cpu.max_guest_page_table_levels());
            if max_levels < self.pt_levels {
                self.pt_levels = max_levels;
            }
        }

        debug!(
            "VM {} ({}) vCPU count: {}, Max Guest Page Table Levels: {}",
            self.config.id, self.config.name, vcpu_count, self.pt_levels
        );
        Ok(vcpus)
    }
}
