use crate::{
    CpuId,
    arch::HCpu,
    data::{VmData, VmDataWeak},
    vhal::cpu::{CpuHardId, HCpuExclusive},
};

#[derive(Debug)]
pub struct VCpuCommon {
    pub(crate) hcpu: HCpuExclusive,
    vm: VmDataWeak,
}

impl VCpuCommon {
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
