use alloc::string::String;
use derive_more::{From, Into};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, From, Into)]
pub struct VmId(usize);

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub id: VmId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VmStatus {
    #[default]
    Uninit,
    Initialized,
    Busy,
    Running,
    Stopped,
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
