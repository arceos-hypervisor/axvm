use crate::{AxVMConfig, VmStatus, hal::ArchOp};

mod init;
mod running;

pub use init::StateInited;
pub use running::StateRunning;

pub enum Machine<H: ArchOp> {
    Uninit(AxVMConfig),
    Initialized(StateInited<H>),
    Running(StateRunning<H>),
    Switch,
    Stopped,
}

impl<H: ArchOp> Machine<H> {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        Ok(Machine::Uninit(config))
    }
}

impl<H: ArchOp> From<&Machine<H>> for VmStatus {
    fn from(machine: &Machine<H>) -> Self {
        match machine {
            Machine::Uninit(_) => VmStatus::Uninit,
            Machine::Initialized(_) => VmStatus::Initialized,
            Machine::Switch => VmStatus::Busy,
            Machine::Running(_) => VmStatus::Running,
            Machine::Stopped => VmStatus::Stopped,
        }
    }
}
