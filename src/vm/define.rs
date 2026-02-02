use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use alloc::string::String;
use derive_more::{From, Into};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, From, Into)]
pub struct VmId(usize);

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub id: VmId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum VMStatus {
    #[default]
    Uninit,
    Initialized,
    Running,
    Stopping,
    Stopped,
}

impl VMStatus {
    pub fn from_u8(value: u8) -> Self {
        unsafe { core::mem::transmute(value) }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("VM exited normally")]
    Exit,
    #[error("VM exited with error: {0}")]
    ExitWithError(#[from] anyhow::Error),
}

impl Clone for RunError {
    fn clone(&self) -> Self {
        match self {
            RunError::Exit => RunError::Exit,
            RunError::ExitWithError(err) => RunError::ExitWithError(anyhow::anyhow!("{err}")),
        }
    }
}

pub(crate) type VmStatistics = alloc::sync::Arc<VmStatisticsInner>;

pub(crate) struct VmStatisticsInner {
    pub status: AtomicStatus,
    pub running_vcpu_count: AtomicUsize,
}

impl Default for VmStatisticsInner {
    fn default() -> Self {
        Self {
            status: AtomicStatus::new(VMStatus::Uninit),
            running_vcpu_count: AtomicUsize::new(0),
        }
    }
}

/// Auxiliary wrapper that stores the current machine status in an atomically
/// readable form so management threads can query it without synchronisation
/// overhead.
pub(crate) struct AtomicStatus(AtomicU8);

impl AtomicStatus {
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
