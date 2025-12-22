use core::ops::Deref;

use alloc::vec::Vec;
use arm_vcpu::Aarch64VCpuSetupConfig;

use crate::{
    AxVMConfig, GuestPhysAddr, VmAddrSpace, VmMachineUninitOps,
    arch::{VmMachineInited, cpu::VCpu},
    config::CpuNumType,
    data::VmDataWeak,
    fdt::FdtBuilder,
};

pub struct VmMachineUninit {
    config: AxVMConfig,
    pt_levels: usize,
    pa_max: usize,
}

impl VmMachineUninitOps for VmMachineUninit {
    type Inited = VmMachineInited;

    fn new(config: AxVMConfig) -> Self {
        Self {
            config,
            pt_levels: 4,
            pa_max: usize::MAX,
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
            let (max_levels, max_pa) =
                vcpu.with_hcpu(|cpu| (cpu.max_guest_page_table_levels(), cpu.pa_range.end));
            if max_levels < self.pt_levels {
                self.pt_levels = max_levels;
            }
            if max_pa < self.pa_max {
                self.pa_max = max_pa;
            }
        }

        if self.pt_levels == 3 {
            self.pa_max = self.pa_max.min(0x8000000000);
        }

        debug!(
            "VM {} ({}) vCPU count: {}, \n  Max Guest Page Table Levels: {}\n  Max PA: {:#x}",
            self.config.id, self.config.name, vcpu_count, self.pt_levels, self.pa_max
        );
        Ok(vcpus)
    }

    fn init_raw(&mut self, vmdata: VmDataWeak) -> anyhow::Result<VmMachineInited> {
        debug!("Initializing VM {} ({})", self.config.id, self.config.name);
        let mut cpus = self.new_vcpus(&vmdata)?;

        let mut vmspace = VmAddrSpace::new(
            self.pt_levels,
            GuestPhysAddr::from_usize(0)..self.pa_max.into(),
        )?;

        debug!(
            "Mapping memory regions for VM {} ({})",
            self.config.id, self.config.name
        );
        for memory_cfg in &self.config.memory_regions {
            vmspace.new_memory(memory_cfg)?;
        }

        vmspace.load_kernel_image(&self.config)?;
        let mut fdt = FdtBuilder::new()?;
        fdt.setup_cpus(cpus.iter().map(|c| c.deref()))?;
        fdt.setup_memory(vmspace.memories().iter())?;
        fdt.setup_chosen(None)?;

        let dtb_data = fdt.build()?;

        let dtb_addr = vmspace.load_dtb(&dtb_data)?;

        vmspace.map_passthrough_regions()?;

        let kernel_entry = vmspace.kernel_entry();
        let gpt_root = vmspace.gpt_root();

        // Setup vCPUs
        for vcpu in &mut cpus {
            vcpu.vcpu.set_entry(kernel_entry).unwrap();
            vcpu.vcpu.set_dtb_addr(dtb_addr).unwrap();
            vcpu.set_pt_level(self.pt_levels);

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
        }

        Ok(VmMachineInited {
            id: self.config.id.into(),
            name: self.config.name.clone(),
            vmspace,
            vcpus: cpus,
        })
    }
}
