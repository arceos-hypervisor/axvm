use std::sync::{Arc, Weak};

use spin::RwLock;

use crate::{
    AxVMConfig,
    arch::Hal,
    define::{VmId, VmInfo, VmState},
    machine::Machine,
};

mod define;
pub mod machine;
pub mod vcpu;

pub use define::*;

pub struct Vm {
    info: VmInfo,
    machine: Arc<RwLock<Machine<Hal>>>,
}

impl Vm {
    pub fn id(&self) -> VmId {
        self.info.id
    }

    pub fn name(&self) -> &str {
        &self.info.name
    }

    pub fn new(config: &AxVMConfig) -> anyhow::Result<Self> {
        let info = VmInfo {
            id: config.id.into(),
            name: config.name.clone(),
        };
        let machine = Arc::new(RwLock::new(Machine::new(config)?));
        Ok(Self { info, machine })
    }

    pub fn downgrade(&self) -> VmWeak {
        VmWeak {
            info: self.info.clone(),
            machine: Arc::downgrade(&self.machine),
        }
    }

    pub fn state(&self) -> anyhow::Result<VmState> {
        let machine = self.machine.read();
        Ok(machine.as_ref().into())
    }
}

#[derive(Clone)]
pub struct VmWeak {
    info: VmInfo,
    machine: Weak<RwLock<Machine<Hal>>>,
}

impl VmWeak {
    pub fn id(&self) -> VmId {
        self.info.id
    }

    pub fn name(&self) -> &str {
        &self.info.name
    }

    pub fn upgrade(&self) -> Option<Vm> {
        let machine = self.machine.upgrade()?;
        Some(Vm {
            info: self.info.clone(),
            machine,
        })
    }
}
