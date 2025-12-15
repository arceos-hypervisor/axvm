use core::fmt;

use alloc::sync::Arc;
use spin::Mutex;
use std::thread;

use crate::{AxVMConfig, arch::VmInit};

mod data;
mod machine;
mod addrspace;
pub(crate) use data::*;
use machine::*;

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
        write!(f, "{:?}", self)
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

pub trait VmStatusInitOps {
    type Running: VmStatusRunningOps;
    fn id(&self) -> VmId;
    fn name(&self) -> &str;
    fn start(self) -> Result<Self::Running, (anyhow::Error, Self)>
    where
        Self: Sized;
}

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("VM exited normally")]
    Exit,
    #[error("VM exited with error: {0}")]
    ExitWithError(#[from] anyhow::Error),
}

pub trait VmStatusRunningOps {
    type Stopping: VmStatusStoppingOps;
    fn do_work(&mut self) -> Result<(), RunError>;
    fn stop(self) -> Result<Self::Stopping, (anyhow::Error, Self)>
    where
        Self: Sized;
}

pub trait VmStatusStoppingOps {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Running,
    ShuttingDown,
    PoweredOff,
}

pub struct Vm {
    handle: VmHandle,
    res: Arc<Mutex<Option<Result<(), RunError>>>>,
}

impl Vm {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let mut arch_vm = VmInit::new(&config)?;
        arch_vm.init(config)?;
        let mut machine = VmMachine::new(arch_vm)?;
        let handle = machine.handle();
        let res = Arc::new(Mutex::new(None));
        let res_arc = res.clone();

        thread::Builder::new()
            .name(format!("{}-main", handle.common.id.0))
            .spawn(move || {
                let res = machine.run();
                let mut guard = res_arc.lock();
                guard.replace(res);
            })
            .map_err(|e| anyhow::anyhow!("Failed to spawn VM thread: {:?}", e))?;

        Ok(Vm { handle, res })
    }

    pub fn id(&self) -> VmId {
        self.handle.common.id
    }

    pub fn name(&self) -> &str {
        &self.handle.common.name
    }

    pub fn boot(&self) -> anyhow::Result<()> {
        self.handle.start()
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        self.handle.shutdown()
    }

    pub fn status(&self) -> VMStatus {
        self.handle.status()
    }

    pub fn wait(&self) -> Result<(), RunError> {
        while !matches!(self.status(), VMStatus::Stopped) {
            thread::sleep(std::time::Duration::from_millis(50));
        }
        let guard = self.res.lock();
        let res = guard.as_ref().unwrap();
        match res {
            Ok(()) => Ok(()),
            Err(e) => match e {
                RunError::Exit => Ok(()),
                RunError::ExitWithError(err) => Err(RunError::ExitWithError(anyhow!("{err}"))),
            },
        }
    }
}
