use crate::{
    CpuId, RunError, VmId, VmWeak,
    arch::HCpu,
    hal::{
        ArchOp,
        cpu::{CpuHardId, HCpuExclusive},
    },
};

pub trait VCpuOp: core::fmt::Debug + Send + 'static {
    fn run(&mut self) -> Result<(), RunError>;
}

pub struct VCpu<H: ArchOp> {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    pub vm: VmWeak,
    pub hcpu_exclusive: HCpuExclusive,
    inner: H::VCPU,
}

impl<H: ArchOp> VCpu<H> {
    pub fn new(bind_id: Option<CpuId>, vm: VmWeak) -> anyhow::Result<Self> {
        let hcpu_exclusive = HCpuExclusive::try_new(bind_id)
            .ok_or_else(|| anyhow!("Failed to allocate hardware CPU"))?;
        let hard_id = hcpu_exclusive.hard_id();
        let id = hcpu_exclusive.id();

        let inner = H::new_vcpu(hard_id.clone(), vm.clone())?;

        Ok(Self {
            id,
            hard_id,
            vm,
            hcpu_exclusive,
            inner,
        })
    }

    pub fn bind_id(&self) -> CpuId {
        self.id
    }
}
