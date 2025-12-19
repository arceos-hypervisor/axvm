use core::{
    fmt::{self, Debug},
    ops::Deref,
};
use std::{
    string::String,
    sync::{Arc, Weak},
};

use spin::RwLock;

use crate::{
    AxVMConfig, RunError, VmId, VmMachineInitedOps, VmMachineRunningOps, VmMachineUninitOps,
    arch::{VmMachineRunning, VmMachineUninit},
    vm::machine::{AtomicState, VMStatus, VmMachineState},
};

pub(crate) struct VmDataInner {
    pub id: VmId,
    pub name: String,
    pub machine: RwLock<VmMachineState>,
    pub status: AtomicState,
    error: RwLock<Option<RunError>>,
}

impl VmDataInner {
    pub fn new(config: AxVMConfig) -> Self {
        Self {
            id: config.id.into(),
            name: config.name.clone(),
            machine: RwLock::new(VmMachineState::Uninit(VmMachineUninit::new(config))),
            status: AtomicState::new(VMStatus::Uninit),
            error: RwLock::new(None),
        }
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let mut status_guard = self.machine.write();
        match core::mem::replace(&mut *status_guard, VmMachineState::Switching) {
            VmMachineState::Running(running) => {
                let stopping = running.stop();
                *status_guard = VmMachineState::Stopping(stopping);
                self.status.store(VMStatus::Stopping);
                Ok(())
            }
            other => {
                *status_guard = other;
                Err(anyhow::anyhow!("VM is not in Running state"))
            }
        }
    }

    pub fn wait(&self) -> anyhow::Result<()> {
        while !matches!(self.status(), VMStatus::Stopped) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.run_result()
    }

    #[inline]
    pub fn status(&self) -> VMStatus {
        self.status.load()
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        let status = self.status();
        status < VMStatus::Stopping
    }

    pub(crate) fn set_err(&self, err: RunError) {
        let mut guard = self.error.write();
        *guard = Some(err);
    }

    pub(crate) fn run_result(&self) -> anyhow::Result<()> {
        let guard = self.error.read();
        let res = guard.clone();
        match res {
            Some(err) => match err {
                RunError::Exit => Ok(()),
                RunError::ExitWithError(e) => Err(e),
            },
            None => Ok(()),
        }
    }
}

pub(crate) struct VmData {
    inner: Arc<VmDataInner>,
}

impl VmData {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Arc::new(VmDataInner::new(config)),
        })
    }

    pub fn id(&self) -> VmId {
        self.inner.id
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn init(&self) -> anyhow::Result<()> {
        let mut status_guard = self.machine.write();
        match core::mem::replace(&mut *status_guard, VmMachineState::Switching) {
            VmMachineState::Uninit(uninit) => {
                let init = match uninit.init(self.downgrade()) {
                    Ok(inited) => inited,
                    Err(e) => {
                        self.set_err(RunError::ExitWithError(anyhow!("{e}")));
                        *status_guard = VmMachineState::Stopped;
                        self.status.store(VMStatus::Stopped);
                        return Err(e);
                    }
                };
                *status_guard = VmMachineState::Inited(init);
                self.status.store(VMStatus::Inited);
                Ok(())
            }
            other => {
                *status_guard = other;
                Err(anyhow::anyhow!("VM is not in Uninit state"))
            }
        }
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let data = self.downgrade();
        let mut status_guard = self.machine.write();
        match core::mem::replace(&mut *status_guard, VmMachineState::Switching) {
            VmMachineState::Inited(init) => match init.start(data) {
                Ok(running) => {
                    *status_guard = VmMachineState::Running(running);
                    self.status.store(VMStatus::Running);
                    Ok(())
                }
                Err(e) => {
                    self.set_err(RunError::ExitWithError(anyhow!("{e}")));
                    *status_guard = VmMachineState::Stopped;
                    self.status.store(VMStatus::Stopped);
                    Err(e)
                }
            },
            other => {
                *status_guard = other;
                Err(anyhow::anyhow!("VM is not in Init state"))
            }
        }
    }

    pub fn downgrade(&self) -> VmDataWeak {
        VmDataWeak {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl From<Arc<VmDataInner>> for VmData {
    fn from(inner: Arc<VmDataInner>) -> Self {
        Self { inner }
    }
}

impl Deref for VmData {
    type Target = VmDataInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Clone)]
pub struct VmDataWeak {
    inner: Weak<VmDataInner>,
}

impl VmDataWeak {
    pub fn upgrade(&self) -> Option<VmData> {
        Some(self.inner.upgrade()?.into())
    }

    pub fn try_upgrade(&self) -> anyhow::Result<VmData> {
        let res = self
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("VM data has been dropped"))?;
        Ok(res)
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        if let Some(inner) = self.upgrade() {
            inner.is_active()
        } else {
            false
        }
    }

    pub(crate) fn set_stopped(&self) {
        if let Some(inner) = self.upgrade() {
            let mut status_guard = inner.machine.write();
            *status_guard = VmMachineState::Stopped;
            inner.status.store(VMStatus::Stopped);
        }
    }

    pub(crate) fn with_machine_running<F, R>(&self, f: F) -> Result<R, RunError>
    where
        F: FnOnce(&VmMachineRunning) -> R,
    {
        let vmdata = self.try_upgrade()?;
        let status = vmdata.machine.read();
        let running = match &*status {
            VmMachineState::Running(running) => running,
            _ => {
                return Err(RunError::ExitWithError(anyhow!(
                    "VM is not in Running state"
                )));
            }
        };
        Ok(f(running))
    }
}

impl Debug for VmDataWeak {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.upgrade() {
            Some(data) => write!(
                f,
                "VmDataWeak {{ id: {}, name: {} }}",
                data.id(),
                data.name()
            ),
            None => write!(f, "VmDataWeak {{ dropped }}"),
        }
    }
}
