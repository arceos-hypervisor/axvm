use crate::{
    CpuId, RunError, VmId,
    arch::HCpu,
    data::{VmData, VmDataWeak},
    hal::cpu::{CpuHardId, HCpuExclusive},
};

pub trait VCpuOp: core::fmt::Debug + Send + 'static {
    fn bind_id(&self) -> CpuId;
    fn hard_id(&self) -> CpuHardId;
    fn run(&mut self) -> Result<(), RunError>;
}

#[derive(Debug)]
pub struct VCpuCommon {
    pub(crate) hcpu: HCpuExclusive,
    vm: VmDataWeak,
}

impl VCpuCommon {
    pub fn vm_id(&self) -> VmId {
        self.vm.id()
    }

    pub fn new_exclusive(bind: Option<CpuId>, vm: VmDataWeak) -> anyhow::Result<Self> {
        let hcpu = HCpuExclusive::try_new(bind)
            .ok_or_else(|| anyhow!("Failed to allocate cpu with id `{bind:?}`"))?;
        Ok(VCpuCommon { hcpu, vm })
    }

    pub fn bind_id(&self) -> CpuId {
        self.hcpu.id()
    }

    pub fn hard_id(&self) -> CpuHardId {
        self.hcpu.hard_id()
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.vm.is_active()
    }

    pub fn with_hcpu<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HCpu) -> R,
    {
        self.hcpu.with_cpu(f)
    }

    pub fn vm(&self) -> anyhow::Result<VmData> {
        self.vm.try_upgrade()
    }
}
