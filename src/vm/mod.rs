use crate::{AxVMConfig, data::VmData};

mod addrspace;
pub(crate) mod data;
mod define;
mod machine;

pub(crate) use addrspace::*;
pub use define::*;
pub(crate) use machine::*;

pub struct Vm {
    data: VmData,
}

impl Vm {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let data = VmData::new(config)?;
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

    pub fn wait(&self) -> anyhow::Result<()> {
        self.data.wait()
    }
}
