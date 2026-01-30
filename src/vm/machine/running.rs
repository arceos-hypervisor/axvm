use crate::hal::ArchOp;

pub struct StateRunning<H: ArchOp> {
    _marker: core::marker::PhantomData<H>,
}

impl<H: ArchOp> StateRunning<H> {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            _marker: core::marker::PhantomData,
        })
    }
}
