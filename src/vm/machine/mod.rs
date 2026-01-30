use crate::{
    AxVMConfig,
    define::VmState,
    hal::ArchOp,
    machine::{init::StateInited, running::StateRunning},
};

mod init;
mod running;

pub enum Machine<H: ArchOp> {
    Initialized(StateInited<H>),
    Running(StateRunning<H>),
    Stopped,
}

impl<H: ArchOp> Machine<H> {
    pub fn new(config: &AxVMConfig) -> anyhow::Result<Self> {
        Ok(Machine::Initialized(StateInited::new(config)?))
    }
}

impl<H: ArchOp> From<&Machine<H>> for VmState {
    fn from(machine: &Machine<H>) -> Self {
        match machine {
            Machine::Initialized(_) => VmState::Initialized,
            Machine::Running(_) => VmState::Running,
            Machine::Stopped => VmState::Stopped,
        }
    }
}
