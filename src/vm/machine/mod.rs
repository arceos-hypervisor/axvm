use alloc::boxed::Box;

use crate::{AxVMConfig, VMStatus, hal::ArchOp};

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
}

impl<H: ArchOp> From<&Machine<H>> for VMStatus {
    fn from(machine: &Machine<H>) -> Self {
        match machine {
            Machine::Uninit(_) => VMStatus::Uninit,
            Machine::Initialized(_) => VMStatus::Initialized,
            Machine::Switch => VMStatus::Busy,
            Machine::Running(_) => VMStatus::Running,
            Machine::Stopped => VMStatus::Stopped,
        }
    }
}
