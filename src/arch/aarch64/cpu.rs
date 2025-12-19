use core::{fmt::Display, ops::Deref};

use aarch64_cpu::registers::*;
use arm_vcpu::{Aarch64PerCpu, Aarch64VCpuCreateConfig};
use axvm_types::addr::*;

use crate::{
    RunError,
    data::VmDataWeak,
    vcpu::{VCpuCommon, VCpuOp},
    vhal::{
        ArchCpuData,
        cpu::{CpuHardId, CpuId},
    },
};

pub struct HCpu {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    vpercpu: Aarch64PerCpu,
    max_guest_page_table_levels: usize,
    pub pa_range: core::ops::Range<usize>,
}

impl HCpu {
    pub fn new(id: CpuId) -> Self {
        let mpidr = MPIDR_EL1.get() as usize;
        let hard_id = mpidr & 0xff_ff_ff;

        let vpercpu = Aarch64PerCpu::new();

        HCpu {
            id,
            hard_id: CpuHardId::new(hard_id),
            vpercpu,
            max_guest_page_table_levels: 0,
            pa_range: 0..0,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        self.vpercpu.hardware_enable();
        self.max_guest_page_table_levels = self.vpercpu.max_guest_page_table_levels();
        self.pa_range = self.vpercpu.pa_range();
        Ok(())
    }

    pub fn max_guest_page_table_levels(&self) -> usize {
        self.max_guest_page_table_levels
    }
}

impl ArchCpuData for HCpu {
    fn hard_id(&self) -> CpuHardId {
        self.hard_id
    }
}

impl Display for HCpu {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "
CPU {}:
  Hard ID: {}
  PT Levels: {}",
            self.id, self.hard_id, self.max_guest_page_table_levels
        )
    }
}

pub(super) struct VCpuHal;

impl arm_vcpu::CpuHal for VCpuHal {
    fn irq_hanlder(&self) {
        axhal::irq::irq_handler(0);
    }

    fn inject_interrupt(&self, irq: usize) {
        todo!()
    }
}

pub struct VCpu {
    pub vcpu: arm_vcpu::Aarch64VCpu,
    common: VCpuCommon,
}

impl VCpu {
    pub fn new(
        host_cpuid: Option<CpuId>,
        dtb_addr: GuestPhysAddr,
        vm: VmDataWeak,
    ) -> anyhow::Result<Self> {
        let common = VCpuCommon::new_exclusive(host_cpuid, vm)?;

        let hard_id = common.hard_id();

        let vcpu = arm_vcpu::Aarch64VCpu::new(Aarch64VCpuCreateConfig {
            mpidr_el1: hard_id.raw() as u64,
            dtb_addr: dtb_addr.as_usize(),
        })
        .unwrap();
        Ok(VCpu { vcpu, common })
    }
}

impl VCpuOp for VCpu {
    fn bind_id(&self) -> CpuId {
        self.common.bind_id()
    }

    fn hard_id(&self) -> CpuHardId {
        self.common.hard_id()
    }

    fn run(&mut self) -> Result<(), RunError> {
        info!("Starting vCPU {}", self.bind_id());

        while self.is_active() {
            let exit_reason = self.vcpu.run().map_err(|e| anyhow!("{e}"))?;
            debug!(
                "vCPU {} exited with reason: {:?}",
                self.bind_id(),
                exit_reason
            );
            match exit_reason {
                arm_vcpu::AxVCpuExitReason::Hypercall { nr, args } => todo!(),
                arm_vcpu::AxVCpuExitReason::MmioRead {
                    addr,
                    width,
                    reg,
                    reg_width,
                    signed_ext,
                } => todo!(),
                arm_vcpu::AxVCpuExitReason::MmioWrite { addr, width, data } => todo!(),
                arm_vcpu::AxVCpuExitReason::SysRegRead { addr, reg } => todo!(),
                arm_vcpu::AxVCpuExitReason::SysRegWrite { addr, value } => todo!(),
                arm_vcpu::AxVCpuExitReason::ExternalInterrupt => {
                    axhal::irq::irq_handler(0);
                }
                arm_vcpu::AxVCpuExitReason::CpuUp {
                    target_cpu,
                    entry_point,
                    arg,
                } => {
                    self.vm()?.with_machine_running_mut(|running| {
                        running.cpu_up(CpuHardId::new(target_cpu as _), entry_point, arg)
                    })??;
                }
                arm_vcpu::AxVCpuExitReason::CpuDown { _state } => todo!(),
                arm_vcpu::AxVCpuExitReason::SystemDown => {
                    info!("vCPU {} requested system shutdown", self.bind_id());
                    self.vm()?.stop()?;
                }
                arm_vcpu::AxVCpuExitReason::Nothing => {}
                arm_vcpu::AxVCpuExitReason::SendIPI {
                    target_cpu,
                    target_cpu_aux,
                    send_to_all,
                    send_to_self,
                    vector,
                } => todo!(),
                _ => todo!(),
            }
        }

        Ok(())
    }
}

impl Deref for VCpu {
    type Target = VCpuCommon;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}
