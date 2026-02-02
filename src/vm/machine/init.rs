use std::vec::Vec;

use crate::{
    AxVMConfig, GuestPhysAddr, VmAddrSpace, VmWeak, config::CpuNumType, fdt::FdtBuilder, hal::{ArchOp, HCpuOp}, machine::running::StateRunning, vcpu::VCpu
};

pub struct StateInited<H: ArchOp> {
    pub vcpus: Vec<VCpu<H>>,
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
}

impl<H: ArchOp> StateInited<H> {
    pub fn new(config: &AxVMConfig, vm: VmWeak) -> anyhow::Result<Self> {
        info!("Initializing VM {} ({})", config.id, config.name);
        // Get vCPU count
        let vcpus = Self::new_vcpus(config, vm)?;
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

        let mut fdt = FdtBuilder::new()?;
        // fdt.setup_cpus(cpus.iter().map(|c| c.deref()))?;
        // fdt.setup_memory(vmspace.memories().iter())?;
        // fdt.setup_chosen(None)?;

        // let dtb_data = fdt.build()?;

        // let dtb_addr = vmspace.load_dtb(&dtb_data)?;

        // vmspace.map_passthrough_regions()?;

        Ok(Self {
            vcpus,
            pt_levels,
            pa_max,
            pa_bits,
        })
    }

    fn new_vcpus(config: &AxVMConfig, vm: VmWeak) -> anyhow::Result<Vec<VCpu<H>>> {
        let mut vcpus = vec![];
        match config.cpu_num {
            CpuNumType::Alloc(num) => {
                for _ in 0..num {
                    let vcpu = VCpu::new(None, vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
            CpuNumType::Fixed(ref ids) => {
                for id in ids {
                    let vcpu = VCpu::new(Some(*id), vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
        }

        let vcpu_count = vcpus.len();
        Ok(vcpus)
    }

    pub fn run(self) -> anyhow::Result<StateRunning<H>> {
        StateRunning::new()
    }
}

#[derive(Debug)]
struct AddrspaceInfo {
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
}

fn calculate_addrspace_info<H: ArchOp>(vcpus: &[VCpu<H>]) -> AddrspaceInfo {
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
