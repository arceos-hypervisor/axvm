use axvmconfig::VMInterruptMode;

use crate::{
    CpuId, GuestPhysAddr, HostPhysAddr, RunError, VmId, VmWeak,
    arch::HCpu,
    hal::{
        ArchOp,
        cpu::{CpuHardId, HCpuExclusive},
    },
};

pub(crate) struct CpuBootInfo {
    pub kernel_entry: GuestPhysAddr,
    pub dtb_addr: GuestPhysAddr,
    pub gpt_root: HostPhysAddr,
    pub pt_levels: usize,
    pub pa_bits: usize,
    pub irq_mode: VMInterruptMode,
}

pub(crate) trait VCpuOp: core::fmt::Debug + Send + 'static {
    fn set_boot_info(&mut self, info: &CpuBootInfo) -> anyhow::Result<()>;
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

    pub fn hcpu(&self) -> &HCpu {
        self.hcpu_exclusive.cpu()
    }

    pub fn set_boot_info(&mut self, info: &CpuBootInfo) -> anyhow::Result<()> {
        self.inner.set_boot_info(info)
    }
}
