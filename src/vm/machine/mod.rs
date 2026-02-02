use alloc::boxed::Box;

use crate::{AxVMConfig, CpuHardId, GuestPhysAddr, VMStatus, hal::ArchOp};

mod init;
mod running;

pub use init::StateInited;
pub use running::StateRunning;

pub enum Machine<H: ArchOp> {
    Uninit(Box<AxVMConfig>),
    Initialized(StateInited<H>),
    Running(StateRunning<H>),
    Switch,
    Stopped,
}

impl<H: ArchOp> Machine<H> {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        Ok(Machine::Uninit(Box::new(config)))
    }

    pub fn cpu_up(
        &mut self,
        target_cpu: CpuHardId,
        entry_point: GuestPhysAddr,
        arg: u64,
    ) -> anyhow::Result<()> {
        todo!()
    }
}
