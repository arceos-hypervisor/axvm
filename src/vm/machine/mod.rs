use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::{
    AxVMConfig, Status, VmId,
    arch::{VmMachineInited, VmMachineRunning, VmMachineUninit, VmStatusStopping},
    data::VmDataWeak,
};

mod running;

pub(crate) use running::*;

pub trait VmMachineUninitOps {
    type Inited: VmMachineInitedOps;
    fn new(config: AxVMConfig) -> Self;
    fn init(self, vmdata: VmDataWeak) -> Result<Self::Inited, anyhow::Error>
    where
        Self: Sized;
}

#[allow(unused)]
pub trait VmMachineInitedOps {
    type Running: VmMachineRunningOps;
    fn id(&self) -> VmId;
    fn name(&self) -> &str;
    fn start(self, vmdata: VmDataWeak) -> Result<Self::Running, anyhow::Error>
    where
        Self: Sized;
}

pub trait VmMachineRunningOps {
    type Stopping: VmMachineStoppingOps;
    fn stop(self) -> Self::Stopping;
}

pub trait VmMachineStoppingOps {}

/// A lightweight container that stores the identifier and human readable name
/// for a VM instance. Shared between the public [`Vm`] object and the
/// background machine thread for logging and observability.
#[derive(Debug, Clone)]
pub struct VmCommon {
    pub id: VmId,
    pub name: String,
}

pub enum VmMachineState {
    Uninit(VmMachineUninit),
    Inited(VmMachineInited),
    Running(VmMachineRunning),
    Switching,
    #[allow(unused)]
    Stopping(VmStatusStopping),
    Stopped,
}

impl VmMachineState {
    pub fn is_active(&self) -> bool {
        !matches!(self, VmMachineState::Stopping(_) | VmMachineState::Stopped)
    }
}

/// Auxiliary wrapper that stores the current machine status in an atomically
/// readable form so management threads can query it without synchronisation
/// overhead.
pub(crate) struct AtomicState(AtomicU8);

impl AtomicState {
    pub const fn new(state: VMStatus) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    #[inline]
    pub fn load(&self) -> VMStatus {
        VMStatus::from_u8(self.0.load(Ordering::Acquire))
    }

    pub fn store(&self, new_state: VMStatus) {
        self.0.store(new_state as u8, Ordering::Release);
    }
}

/// High-level VM lifecycle that is visible to callers of the [`Vm`] API.
/// This is intentionally richer than the low-level `Status` that is returned
/// by the architecture specific implementation so that the shell and
/// management layers can express user-friendly states.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum VMStatus {
    #[default]
    Uninit,
    Switching,
    Inited,
    Running,
    Suspended,
    Stopping,
    Stopped,
}

impl VMStatus {
    fn from_u8(raw: u8) -> Self {
        unsafe { core::mem::transmute(raw) }
    }
}

impl From<Status> for VMStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::Idle => VMStatus::Inited,
            Status::Running => VMStatus::Running,
            Status::ShuttingDown => VMStatus::Stopping,
            Status::PoweredOff => VMStatus::Stopped,
        }
    }
}
