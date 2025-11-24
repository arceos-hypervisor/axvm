use alloc::string::String;
use spin::Mutex;

use crate::AxVMConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VmId(usize);

impl VmId {
    pub fn new(id: usize) -> Self {
        VmId(id)
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

pub trait VmOps {
    fn id(&self) -> VmId;
    fn name(&self) -> &str;
    fn boot(&mut self) -> anyhow::Result<()>;
    fn stop(&self);
    fn status(&self) -> Status;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Running,
    ShuttingDown,
    PoweredOff,
}

pub struct Vm {
    id: VmId,
    name: String,
    inner: Mutex<crate::arch::ArchVm>,
}

impl Vm {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let mut arch_vm = crate::arch::ArchVm::new(config)?;
        arch_vm.init()?;

        Ok(Vm {
            id: arch_vm.id(),
            name: arch_vm.name().into(),
            inner: Mutex::new(arch_vm),
        })
    }

    pub fn id(&self) -> VmId {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn boot(&self) -> anyhow::Result<()> {
        let mut arch_vm = self.inner.lock();
        arch_vm.boot()
    }
}
