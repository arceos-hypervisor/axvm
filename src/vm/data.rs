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
    pub memory_size: usize,
    pub vcpu_num: usize,
    error: RwLock<Option<RunError>>,
}

impl VmDataInner {
    pub fn new(config: AxVMConfig) -> Self {
        // Calculate total memory size
        let memory_size = config
            .memory_regions
            .iter()
            .map(|region| match region {
                crate::config::MemoryKind::Identical { size } => *size,
                crate::config::MemoryKind::Reserved { size, .. } => *size,
                crate::config::MemoryKind::Vmem { size, .. } => *size,
            })
            .sum();

        // Get vCPU count
        let vcpu_num = config.cpu_num.num();

        Self {
            id: config.id.into(),
            name: config.name.clone(),
            machine: RwLock::new(VmMachineState::Uninit(VmMachineUninit::new(config))),
            status: AtomicState::new(VMStatus::Uninit),
            error: RwLock::new(None),
            memory_size,
            vcpu_num,
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
            // TODO: arceos bug, sleep never wakes up
            // std::thread::sleep(std::time::Duration::from_millis(50));
            std::thread::yield_now();
        }
        info!("VM {} ({}) has stopped.", self.id, self.name);
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
        let next;
        let res;
        let next_state;

        match self.replace_status(VmMachineState::Switching) {
            VmMachineState::Uninit(uninit) => {
                match uninit.init(self.downgrade()) {
                    Ok(inited) => {
                        next_state = Some(VMStatus::Inited);
                        res = Ok(());
                        next = VmMachineState::Inited(inited);
                    }
                    Err(e) => {
                        self.set_err(RunError::ExitWithError(anyhow!("{e}")));
                        next_state = Some(VMStatus::Stopped);
                        next = VmMachineState::Stopped;
                        res = Err(e);
                    }
                };
            }
            other => {
                next = other;
                next_state = None;
                res = Err(anyhow::anyhow!("VM is not in Uninit state"));
            }
        }
        self.replace_status(next);
        if let Some(status) = next_state {
            self.status.store(status);
        }
        res
    }

    fn replace_status(&self, new_status: VmMachineState) -> VmMachineState {
        let mut status_guard = self.machine.write();
        core::mem::replace(&mut *status_guard, new_status)
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let data = self.downgrade();
        let next_state;
        let res;
        let next = match self.replace_status(VmMachineState::Switching) {
            VmMachineState::Inited(init) => match init.start(data) {
                Ok(running) => {
                    next_state = Some(VMStatus::Running);
                    res = Ok(());
                    VmMachineState::Running(running)
                }
                Err(e) => {
                    self.set_err(RunError::ExitWithError(anyhow!("{e}")));

                    next_state = Some(VMStatus::Stopped);
                    res = Err(e);
                    VmMachineState::Stopped
                }
            },
            other => {
                next_state = None;

                res = Err(anyhow::anyhow!("VM is not in Init state"));
                other
            }
        };
        self.replace_status(next);
        if let Some(status) = next_state {
            self.status.store(status);
        }
        res
    }

    pub fn downgrade(&self) -> VmDataWeak {
        VmDataWeak {
            id: self.id(),
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub(crate) fn with_machine_running<F, R>(&self, f: F) -> Result<R, RunError>
    where
        F: FnOnce(&VmMachineRunning) -> R,
    {
        loop {
            let status = self.machine.read();
            let running = match &*status {
                VmMachineState::Running(running) => running,
                VmMachineState::Switching => {
                    drop(status);
                    std::thread::yield_now();
                    continue;
                }
                _ => {
                    return Err(RunError::ExitWithError(anyhow!(
                        "VM is not in Running state"
                    )));
                }
            };
            return Ok(f(running));
        }
    }

    pub(crate) fn with_machine_running_mut<F, R>(&self, f: F) -> Result<R, RunError>
    where
        F: FnOnce(&mut VmMachineRunning) -> R,
    {
        loop {
            let mut status = self.machine.write();
            let running = match &mut *status {
                VmMachineState::Running(running) => running,
                VmMachineState::Switching => {
                    drop(status);
                    std::thread::yield_now();
                    continue;
                }
                _ => {
                    return Err(RunError::ExitWithError(anyhow!(
                        "VM is not in Running state"
                    )));
                }
            };
            return Ok(f(running));
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
    id: VmId,
    inner: Weak<VmDataInner>,
}

impl VmDataWeak {
    pub fn id(&self) -> VmId {
        self.id
    }

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

    pub(crate) fn wait_for_running(&self) {
        while let Some(inner) = self.upgrade() {
            let status = inner.status.load();
            if status >= VMStatus::Running {
                break;
            }
            std::thread::yield_now();
        }
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
