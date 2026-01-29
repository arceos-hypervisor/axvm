use fdt_edit::NodeRef;

use crate::{
    GuestPhysAddr, VmAddrSpace, VmMachineRunningCommon, VmMachineRunningOps, VmMachineStoppingOps,
    arch::vm::DevMapConfig, hal::cpu::CpuHardId,
};

/// Data needed when VM is running
pub struct VmMachineRunning {
    pub(super) common: VmMachineRunningCommon,
}

impl VmMachineRunning {
    fn handle_node_regs(dev_vec: &mut [DevMapConfig], node: &NodeRef<'_>) {}

    pub fn cpu_up(
        &mut self,
        target_cpu: CpuHardId,
        entry_point: GuestPhysAddr,
        arg: u64,
    ) -> anyhow::Result<()> {
        let mut cpu = self
            .common
            .cpus
            .remove(&target_cpu)
            .ok_or(anyhow!("No cpu {target_cpu} found"))?;

        cpu.vcpu.set_entry(entry_point.as_usize().into()).unwrap();
        cpu.vcpu.set_gpr(0, arg as _);
        self.common.run_cpu(cpu)?;
        Ok(())
    }
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
