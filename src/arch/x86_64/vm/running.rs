use core::ops::Deref;

use crate::{
    GuestPhysAddr, VmMachineRunningCommon, VmMachineRunningOps, VmMachineStoppingOps,
    arch::cpu::VCpu, vhal::cpu::CpuHardId,
};

pub struct VmMachineRunning {
    pub common: VmMachineRunningCommon,
}

impl VmMachineRunning {
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

        // x86 使用 SIPI (Startup IPI) 来启动 AP
        // 这里设置 entry point 和参数
        cpu.vcpu.set_entry(entry_point.as_usize().into())?;
        cpu.vcpu.set_gpr(0, arg as _);
        self.common.run_cpu(cpu)?;
        Ok(())
    }
}

impl Deref for VmMachineRunning {
    type Target = VmMachineRunningCommon;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

impl VmMachineRunningOps for VmMachineRunning {
    type Stopping = super::stopping::VmStatusStopping;

    fn stop(self) -> Self::Stopping {
        debug!("Stopping x86_64 VM");
        super::stopping::VmStatusStopping {}
    }
}
