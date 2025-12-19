use std::{string::String, vec::Vec};

use crate::{
    VmAddrSpace, VmMachineInitedOps, VmMachineRunningCommon,
    arch::{VmMachineRunning, cpu::VCpu},
    data::VmDataWeak,
    vm::VmId,
};

pub struct VmMachineInited {
    pub id: VmId,
    pub name: String,
    pub vcpus: Vec<VCpu>,
    pub vmspace: VmAddrSpace,
}

impl VmMachineInited {}

impl VmMachineInitedOps for VmMachineInited {
    type Running = VmMachineRunning;

    fn id(&self) -> VmId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn start(self, vmdata: VmDataWeak) -> Result<Self::Running, anyhow::Error> {
        debug!("Starting VM {} ({})", self.id, self.name);
        let mut running = VmMachineRunning {
            common: VmMachineRunningCommon::new(self.vmspace, self.vcpus, vmdata),
        };

        let main = running.common.take_cpu()?;

        running.common.run_cpu(main)?;

        info!("VM {} ({}) main cpu started.", self.id, self.name,);
        Ok(running)
    }
}
