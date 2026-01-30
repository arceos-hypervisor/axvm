use std::vec::Vec;

use crate::{
    AxVMConfig, VmWeak, config::CpuNumType, hal::ArchOp, machine::running::StateRunning, vcpu::VCpu,
};

pub struct StateInited<H: ArchOp> {
    pub vcpus: Vec<VCpu<H>>,
}

impl<H: ArchOp> StateInited<H> {
    pub fn new(config: &AxVMConfig, vm: VmWeak) -> anyhow::Result<Self> {
        // Get vCPU count
        let vcpus = Self::new_vcpus(config, vm)?;

        Ok(Self { vcpus })
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
