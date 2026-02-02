use core::{
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};
use std::sync::{Arc, Weak};

use spin::RwLock;

use crate::{
    AxVMConfig, CpuHardId, GuestPhysAddr,
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
    pub(crate) machine: Arc<RwLock<Machine<Hal>>>,
    pub(crate) stat: VmStatistics,
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
        let stat = Arc::new(VmStatisticsInner::default());
        let mut vm = Self {
            info,
            machine,
            stat,
        };
        vm.init()?;

        Ok(vm)
    }

    pub fn is_active(&self) -> bool {
        self.status() < VMStatus::Stopping
    }

    fn init(&mut self) -> anyhow::Result<()> {
        let weak = self.downgrade();

        let mut machine = self.machine.write();
        let old = core::mem::replace(machine.deref_mut(), Machine::Switch);

        let Machine::Uninit(config) = old else {
            bail!("VM is not in uninitialized state");
        };

        let inited = StateInited::new(&config, weak)?;
        *machine = Machine::Initialized(inited);
        self.stat.status.store(VMStatus::Initialized);

        Ok(())
    }

    pub fn downgrade(&self) -> VmWeak {
        VmWeak {
            info: self.info.clone(),
            machine: Arc::downgrade(&self.machine),
            stat: self.stat.clone(),
        }
    }

    pub fn boot(&self) -> anyhow::Result<()> {
        let mut machine = self.machine.write();
        let old = core::mem::replace(machine.deref_mut(), Machine::Switch);

        let Machine::Initialized(inited) = old else {
            bail!("VM is not in initialized state");
        };

        let running = inited.run()?;
        *machine = Machine::Running(running);
        self.stat.status.store(VMStatus::Running);

        Ok(())
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        self.set_exit(None);
        Ok(())
    }

    pub fn wait(&self) -> anyhow::Result<()> {
        while self.status() < VMStatus::Stopped {
            std::thread::yield_now();
        }
        let machine = self.machine.read();
        if let Machine::Stopped(Some(err)) = machine.deref() {
            return Err(anyhow!("VM exited with error: {}", err));
        }
        Ok(())
    }

    pub fn vcpu_num(&self) -> usize {
        self.stat.running_vcpu_count.load(Ordering::Acquire)
    }

    pub fn memory_size(&self) -> usize {
        let machine = self.machine.read();
        // machine.memory_size()
        todo!()
    }

    pub fn status(&self) -> VMStatus {
        self.stat.status.load()
    }

    pub fn set_exit(&self, err: Option<RunError>) {
        let mut machine = self.machine.write();
        if matches!(
            machine.deref(),
            Machine::Stopping { .. } | Machine::Stopped(_)
        ) {
            return;
        }

        let old = core::mem::replace(machine.deref_mut(), Machine::Switch);

        match old {
            Machine::Running(running) => {
                *machine = Machine::Stopping {
                    run: Some(running),
                    err,
                };
            }

            other => {
                *machine = Machine::Stopping { run: None, err };
            }
        }
        self.stat.status.store(VMStatus::Stopping);
    }

    pub(crate) fn cpu_up(
        &self,
        target_cpu: CpuHardId,
        entry_point: GuestPhysAddr,
        arg: usize,
    ) -> anyhow::Result<()> {
        let mut machine = self.machine.write();
        machine.cpu_up(target_cpu, entry_point, arg)
    }
}

#[derive(Clone)]
pub struct VmWeak {
    info: VmInfo,
    machine: Weak<RwLock<Machine<Hal>>>,
    pub(crate) stat: VmStatistics,
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
            stat: self.stat.clone(),
        })
    }

    pub fn status(&self) -> VMStatus {
        self.stat.status.load()
    }

    pub fn wait_for_running(&self) {
        while self.stat.status.load() < VMStatus::Running {
            std::thread::yield_now();
        }
    }

    pub fn set_exit(&self, err: Option<RunError>) {
        if let Some(vm) = self.upgrade() {
            vm.set_exit(err);
        }
    }

    pub fn running_cpu_count(&self) -> usize {
        self.stat.running_vcpu_count.load(Ordering::Acquire)
    }
}
