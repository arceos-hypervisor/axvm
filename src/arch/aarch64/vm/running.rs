use fdt_edit::NodeRef;

use crate::{
    VmAddrSpace, VmMachineRunningCommon, VmMachineRunningOps, VmMachineStoppingOps,
    arch::vm::DevMapConfig,
};

/// Data needed when VM is running
pub struct VmMachineRunning {
    pub(super) common: VmMachineRunningCommon,
}

impl VmMachineRunning {
    fn handle_node_regs(dev_vec: &mut [DevMapConfig], node: &NodeRef<'_>) {}
}

impl VmMachineRunningOps for VmMachineRunning {
    type Stopping = VmStatusStopping;

    fn stop(self) -> Self::Stopping {
        Self::Stopping {
            _vmspace: self.common.vmspace,
        }
    }
}

pub struct VmStatusStopping {
    _vmspace: VmAddrSpace,
}

impl VmMachineStoppingOps for VmStatusStopping {}
