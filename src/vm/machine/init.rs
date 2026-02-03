use core::sync::atomic::{AtomicBool, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    sync::Arc,
    thread::JoinHandle,
    vec::Vec,
};

use axvdev::VDeviceManager;

use crate::{
    AxVMConfig, GuestPhysAddr, TASK_STACK_SIZE, VmAddrSpace, VmWeak,
    arch::PlatData,
    config::CpuNumType,
    fdt::FdtBuilder,
    hal::{HCpuOp, HalOp},
    machine::running::StateRunning,
    vcpu::{CpuBootInfo, VCpu},
};

pub struct StateInited<H: HalOp> {
    pub vcpus: Vec<VCpu<H>>,
    vmspace: VmAddrSpace,
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
    vm: VmWeak,
    plat: H::PlatData,
}

impl<H: HalOp> StateInited<H> {
    pub fn new(config: &AxVMConfig, vm: VmWeak) -> anyhow::Result<Self> {
        info!("Initializing VM {} ({})", config.id, config.name);
        let vdev_manager = VDeviceManager::new();

        let mut plat = H::new_plat_data(&vdev_manager)?;

        // Get vCPU count
        let mut vcpus = Self::new_vcpus(config, vm.clone(), &mut plat)?;
        let addrspace_info = calculate_addrspace_info(&vcpus);
        let pt_levels = addrspace_info.pt_levels;
        let pa_max = addrspace_info.pa_max;
        let pa_bits = addrspace_info.pa_bits;
        debug!(
            "VM {} ({}) \n  vCPU count: {}, \n  Max Guest Page Table Levels: {}\n  Max PA: {:#x}\n  PA Bits: {}",
            config.id,
            config.name,
            vcpus.len(),
            pt_levels,
            pa_max,
            pa_bits
        );

        let mut vmspace = VmAddrSpace::new(pt_levels, GuestPhysAddr::from_usize(0)..pa_max.into())?;
        debug!(
            "Mapping memory regions for VM {} ({})",
            config.id, config.name
        );

        for memory_cfg in &config.memory_regions {
            vmspace.new_memory(memory_cfg)?;
        }
        vmspace.load_kernel_image(&config)?;

        debug!("Setup FDT for VM {} ({})", config.id, config.name);
        let mut fdt = FdtBuilder::new()?;
        fdt.setup_cpus(&vcpus)?;
        vmspace.with_memories(|memores| fdt.setup_memory(memores.iter()))?;

        fdt.setup_chosen(None)?;

        let dtb_data = fdt.build()?;

        let dtb_addr = vmspace.load_dtb(&dtb_data)?;

        vmspace.map_passthrough_regions()?;
        let kernel_entry = vmspace.kernel_entry();
        let gpt_root = vmspace.gpt_root();

        for vcpu in &mut vcpus {
            vcpu.set_boot_info(&CpuBootInfo {
                kernel_entry,
                dtb_addr,
                pt_levels,
                pa_bits,
                irq_mode: config.interrupt_mode(),
                gpt_root,
                secondary_boot_arg: None,
            })?;
        }

        Ok(Self {
            vcpus,
            pt_levels,
            pa_max,
            pa_bits,
            vmspace,
            vm,
            plat,
        })
    }

    fn new_vcpus(
        config: &AxVMConfig,
        vm: VmWeak,
        plat: &mut H::PlatData,
    ) -> anyhow::Result<Vec<VCpu<H>>> {
        let mut vcpus = vec![];
        match config.cpu_num {
            CpuNumType::Alloc(num) => {
                for _ in 0..num {
                    let vcpu = VCpu::new(None, vm.clone(), plat)?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
            CpuNumType::Fixed(ref ids) => {
                for id in ids {
                    let vcpu = VCpu::new(Some(*id), vm.clone(), plat)?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
        }

        let vcpu_count = vcpus.len();
        Ok(vcpus)
    }

    pub fn run(mut self) -> anyhow::Result<StateRunning<H>> {
        debug!("Starting VM {:?} ({})", self.vm.id(), self.vm.name());
        if self.vcpus.is_empty() {
            bail!("No vCPU available to run the VM");
        }

        let mut main = self.vcpus.remove(0);

        let main = self.run_cpu(main)?;

        StateRunning::new(main, self.vcpus, self.vmspace, self.vm, self.plat)
    }

    fn run_cpu(&mut self, mut cpu: VCpu<H>) -> anyhow::Result<JoinHandle<VCpu<H>>> {
        let vm = self.vm.clone();
        cpu.run_in_thread(vm, true)
    }
}

#[derive(Debug)]
struct AddrspaceInfo {
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
}

fn calculate_addrspace_info<H: HalOp>(vcpus: &[VCpu<H>]) -> AddrspaceInfo {
    let mut info = AddrspaceInfo {
        pt_levels: 4,
        pa_max: usize::MAX,
        pa_bits: usize::MAX,
    };

    for vcpu in vcpus {
        let max_levels = vcpu.hcpu().max_guest_page_table_levels();
        let max_pa = vcpu.hcpu().pa_range().end;
        let pa_bits = vcpu.hcpu().pa_bits();

        if max_levels < info.pt_levels {
            info.pt_levels = max_levels;
        }
        if max_pa < info.pa_max {
            info.pa_max = max_pa;
        }

        if pa_bits < info.pa_bits {
            info.pa_bits = pa_bits;
        }
    }

    if info.pt_levels == 3 {
        info.pa_max = info.pa_max.min(0x8000000000);
    }

    info
}
