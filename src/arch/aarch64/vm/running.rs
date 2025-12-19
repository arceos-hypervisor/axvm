use fdt_edit::NodeRef;

use crate::{VmMachineRunningOps, VmMachineStoppingOps, arch::vm::DevMapConfig};

/// Data needed when VM is running
pub struct VmMachineRunning {}

impl VmMachineRunning {
    pub(crate) fn new() -> Self {
        Self {}
    }

    fn handle_node_regs(dev_vec: &mut [DevMapConfig], node: &NodeRef<'_>) {}
}

impl VmMachineRunningOps for VmMachineRunning {
    type Stopping = VmStatusStopping;

    fn stop(self) -> Self::Stopping {
        Self::Stopping {}
    }
}

pub struct VmStatusStopping {}

impl VmMachineStoppingOps for VmStatusStopping {}
