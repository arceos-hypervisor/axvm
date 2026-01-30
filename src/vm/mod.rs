use core::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};

use spin::RwLock;

use crate::{
    AxVMConfig,
    arch::Hal,
    machine::{Machine, StateInited},
};

mod addrspace;
mod define;
pub mod machine;
pub mod vcpu;

pub use addrspace::*;
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

    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let info = VmInfo {
            id: config.id.into(),
            name: config.name.clone(),
        };
        let machine = Arc::new(RwLock::new(Machine::new(config)?));
        Ok(Self { info, machine })
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        let weak = self.downgrade();
        let mut machine = self.machine.write();
        let old = core::mem::replace(machine.deref_mut(), Machine::Switch);

        let Machine::Uninit(config) = old else {
            bail!("VM is not in uninitialized state");
        };

        let inited = StateInited::new(&config, weak)?;
        *machine = Machine::Initialized(inited);

        Ok(())
    }

    pub fn downgrade(&self) -> VmWeak {
        VmWeak {
            info: self.info.clone(),
            machine: Arc::downgrade(&self.machine),
        }
    }

    pub fn boot(&mut self) -> anyhow::Result<()> {
        let mut machine = self.machine.write();
        let old = core::mem::replace(machine.deref_mut(), Machine::Switch);

        let Machine::Initialized(inited) = old else {
            bail!("VM is not in initialized state");
        };

        let running = inited.run()?;
        *machine = Machine::Running(running);

        Ok(())
    }

    pub fn status(&self) -> anyhow::Result<VmStatus> {
        let machine = self.machine.read();
        Ok(VmStatus::from(machine.deref()))
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
