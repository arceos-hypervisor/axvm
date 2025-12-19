use core::fmt;

use alloc::sync::Arc;
use spin::{Mutex, RwLock};
use std::thread;

use crate::{AxVMConfig, arch::VmMachineInited, vm::data2::VmDataWeak};

mod addrspace;
mod data;
pub(crate) mod data2;
mod define;
mod machine;

pub(crate) use data::*;
pub use define::*;
pub(crate) use machine::*;

pub struct Vm {
    data: data2::VmData,
}

impl Vm {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let data = data2::VmData::new(&config)?;
        data.init()?;
        Ok(Self { data })
    }

    pub fn id(&self) -> VmId {
        self.data.id()
    }

    pub fn name(&self) -> &str {
        self.data.name()
    }

    pub fn boot(&self) -> anyhow::Result<()> {
        self.data.start()
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        self.data.stop()
    }

    #[inline]
    pub fn status(&self) -> VMStatus {
        self.data.status()
    }

    pub fn wait(&self) -> Result<(), RunError> {
        while !matches!(self.status(), VMStatus::Stopped) {
            thread::sleep(std::time::Duration::from_millis(50));
        }
        self.data.run_result()
    }
}
