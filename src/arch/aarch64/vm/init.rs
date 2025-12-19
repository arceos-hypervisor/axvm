use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    string::String,
    vec::Vec,
};

use arm_vcpu::Aarch64VCpuSetupConfig;

use crate::{
    GuestPhysAddr, TASK_STACK_SIZE, VmAddrSpace, VmMachineInitedOps,
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

    fn start(self, vmdata: VmDataWeak) -> Result<Self::Running, (anyhow::Error, Self)> {
        debug!("Starting VM {} ({})", self.id, self.name);
        let running = VmMachineRunning::new();
        info!(
            "VM {} ({}) with {} cpus booted successfully.",
            self.id,
            self.name,
            self.vcpus.len()
        );
        Ok(running)
    }
}
