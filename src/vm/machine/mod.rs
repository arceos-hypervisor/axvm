use alloc::boxed::Box;

use crate::{AxVMConfig, CpuHardId, GuestPhysAddr, RunError, hal::ArchOp, vcpu::CpuBootInfo};

mod init;
mod running;

pub use init::StateInited;
pub use running::StateRunning;

pub enum Machine<H: ArchOp> {
    Uninit(Box<AxVMConfig>),
    Initialized(StateInited<H>),
    Running(StateRunning<H>),
    Switch,
    Stopping {
        run: Option<StateRunning<H>>,
        err: Option<RunError>,
    },
    Stopped(Option<RunError>),
}

impl<H: ArchOp> Machine<H> {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        Ok(Machine::Uninit(Box::new(config)))
    }

    pub fn cpu_up(
        &self,
        target_cpu: CpuHardId,
        entry_point: GuestPhysAddr,
        arg: usize,
    ) -> anyhow::Result<()> {
        let Machine::Running(running) = self else {
            bail!("VM is not in running state");
        };
        running.vcpus.cpu_up(target_cpu, entry_point, arg)
    }
}
