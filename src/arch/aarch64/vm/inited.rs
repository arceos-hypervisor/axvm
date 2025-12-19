use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    string::String,
    vec::Vec,
};

use arm_vcpu::Aarch64VCpuSetupConfig;

use crate::{
    GuestPhysAddr, TASK_STACK_SIZE, VmAddrSpace, VmMachineInitedOps, VmMachineRunningCommon,
    arch::{VmMachineRunning, cpu::VCpu},
    config::AxVMConfig,
    data::VmDataWeak,
    vm::VmId,
};

const VM_ASPACE_BASE: GuestPhysAddr = GuestPhysAddr::from_usize(0);
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;
const VM_ASPACE_END: GuestPhysAddr =
    GuestPhysAddr::from_usize(VM_ASPACE_BASE.as_usize() + VM_ASPACE_SIZE);

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
