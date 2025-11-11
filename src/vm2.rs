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
