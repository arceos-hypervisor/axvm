use alloc::vec::Vec;
use axvmconfig::VMInterruptMode;
use core::fmt::{self, Debug, Display};

use aarch64_cpu::registers::*;
use arm_vcpu::{Aarch64PerCpu, Aarch64VCpuCreateConfig, Aarch64VCpuSetupConfig};

use crate::{
    RunError, Vm, VmWeak,
    hal::{
        HCpuOp,
        cpu::{CpuHardId, CpuId},
    },
    vcpu::{CpuBootInfo, VCpuOp},
};

pub struct HCpu {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    vpercpu: Aarch64PerCpu,
    max_guest_page_table_levels: usize,
    pub pa_range: core::ops::Range<usize>,
    pub pa_bits: usize,
}

impl HCpuOp for HCpu {
    fn hard_id(&self) -> CpuHardId {
        self.hard_id
    }

    fn max_guest_page_table_levels(&self) -> usize {
        self.max_guest_page_table_levels
    }

    fn pa_range(&self) -> core::ops::Range<usize> {
        self.pa_range.clone()
    }

    fn pa_bits(&self) -> usize {
        self.pa_bits
    }
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
            pa_bits: 0,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        self.vpercpu.hardware_enable();
        self.max_guest_page_table_levels = self.vpercpu.max_guest_page_table_levels();
        self.pa_range = self.vpercpu.pa_range();
        self.pa_bits = self.vpercpu.pa_bits();
        Ok(())
    }

    pub fn max_guest_page_table_levels(&self) -> usize {
        self.max_guest_page_table_levels
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

    fn cpu_list(&self) -> Vec<usize> {
        crate::hal::cpu::list()
            .into_iter()
            .map(|id| id.raw())
            .collect()
    }
}

pub struct CPUState {
    pub vcpu: arm_vcpu::Aarch64VCpu,
    mpidr_el1: u64,
    boot: CpuBootInfo,
}

impl CPUState {
    pub fn new(id: CpuHardId, vm: VmWeak) -> anyhow::Result<Self> {
        let vcpu = arm_vcpu::Aarch64VCpu::new(Aarch64VCpuCreateConfig {
            mpidr_el1: id.raw() as u64,
            dtb_addr: 0,
        })
        .unwrap();
        Ok(CPUState {
            vcpu,
            mpidr_el1: id.raw() as u64,
            boot: CpuBootInfo::default(),
        })
    }

    pub fn set_pt_level(&mut self, level: usize) {
        self.vcpu.pt_level = level;
    }

    pub fn set_pa_bits(&mut self, pa_bits: usize) {
        self.vcpu.pa_bits = pa_bits;
    }
}

impl VCpuOp for CPUState {
    fn run(&mut self, vm: &Vm) -> Result<(), RunError> {
        info!("Starting vCPU {}", self.mpidr_el1);

        self.vcpu
            .setup_current_cpu(vm.id().into())
            .map_err(|e| anyhow!("{e}"))?;
        while vm.is_active() {
            debug!("vCPU {:#x} entering guest", self.mpidr_el1);
            let exit_reason = self.vcpu.run().map_err(|e| anyhow!("{e}"))?;
            debug!(
                "vCPU {:#x} exited with reason: {:?}",
                self.mpidr_el1, exit_reason
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
                arm_vcpu::AxVCpuExitReason::SysRegRead { addr, reg } => {
                    todo!()
                },
                arm_vcpu::AxVCpuExitReason::SysRegWrite { addr, value } => todo!(),
                arm_vcpu::AxVCpuExitReason::ExternalInterrupt => {
                    axhal::irq::irq_handler(0);
                }
                arm_vcpu::AxVCpuExitReason::CpuUp {
                    target_cpu,
                    entry_point,
                    arg,
                } => {
                    debug!("vCPU {:#x} requested CPU {} up", self.mpidr_el1, target_cpu);
                    vm.cpu_up(CpuHardId::new(target_cpu as _), entry_point, arg as _)?;
                    self.vcpu.set_gpr(0, 0);
                }
                arm_vcpu::AxVCpuExitReason::CpuDown { _state } => todo!(),
                arm_vcpu::AxVCpuExitReason::SystemDown => {
                    info!("vCPU {:#x} requested system shutdown", self.mpidr_el1);
                    // self.vm()?.stop()?;
                    vm.set_exit(None);
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

    fn set_boot_info(&mut self, info: &crate::vcpu::CpuBootInfo) -> anyhow::Result<()> {
        self.vcpu
            .set_entry(info.kernel_entry.as_usize().into())
            .map_err(|e| anyhow!("Failed to set entry {e}"))?;
        self.vcpu
            .set_dtb_addr(info.dtb_addr.as_usize().into())
            .map_err(|e| anyhow!("Failed to set dtb addr {e}"))?;
        self.vcpu.pt_level = info.pt_levels;
        self.vcpu.pa_bits = info.pa_bits;

        let setup_config = Aarch64VCpuSetupConfig {
            passthrough_interrupt: info.irq_mode == axvmconfig::VMInterruptMode::Passthrough,
            passthrough_timer: info.irq_mode == axvmconfig::VMInterruptMode::Passthrough,
        };

        self.vcpu
            .setup(setup_config)
            .map_err(|e| anyhow!("Failed to setup vCPU : {e:?}"))?;

        // Set EPT root
        self.vcpu
            .set_ept_root(info.gpt_root.as_usize().into())
            .map_err(|e| anyhow!("Failed to set EPT root for vCPU : {e:?}"))?;

        if let Some(arg) = info.secondary_boot_arg {
            self.vcpu.set_gpr(0, arg);
        }

        self.boot = info.clone();
        Ok(())
    }

    fn get_boot_info(&self) -> crate::vcpu::CpuBootInfo {
        self.boot.clone()
    }
}

impl Debug for CPUState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VCpu").field("vcpu", &self.vcpu).finish()
    }
}
