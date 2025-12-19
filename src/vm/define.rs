use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmId(usize);

impl VmId {
    pub fn new_fixed(id: usize) -> Self {
        VmId(id)
    }

    pub fn new() -> Self {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static VM_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);
        let id = VM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        VmId(id)
    }
}

impl Default for VmId {
    fn default() -> Self {
        VmId::new()
    }
}

// Implement Display for VmId
impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for VmId {
    fn from(value: usize) -> Self {
        VmId(value)
    }
}

impl From<VmId> for usize {
    fn from(value: VmId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Running,
    ShuttingDown,
    PoweredOff,
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
            RunError::ExitWithError(err) => {
                RunError::ExitWithError(anyhow::anyhow!(format!("{err}")))
            }
        }
    }
}
