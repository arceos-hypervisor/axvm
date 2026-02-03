use core::{
    fmt::{self, Debug, Display},
    ops::Deref,
};

use riscv_vcpu::{RISCVPerCpu, RISCVVCpu, RISCVVCpuCreateConfig};
use axvm_types::addr::*;

use crate::{
    RunError,
    data::VmDataWeak,
    vcpu::{VCpuCommon, VCpuOp},
    hal::{
        HCpuOp,
        cpu::{CpuHardId, CpuId},
    },
};

pub struct HCpu {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    vpercpu: RISCVPerCpu,
    max_guest_page_table_levels: usize,
    pub pa_range: core::ops::Range<usize>,
    pub pa_bits: usize,
}

impl HCpu {
    pub fn new(id: CpuId) -> Self {
        // Get hart ID from percpu storage (set by axplat during boot).
        // The hart ID is passed in a0 register at boot and stored by axplat::init_percpu.
        let hard_id = axplat::percpu::this_cpu_id();

        let vpercpu = RISCVPerCpu::new().expect("Failed to create RISCVPerCpu");

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
        self.vpercpu.hardware_enable()
            .map_err(|e| anyhow::anyhow!("hardware_enable failed: {:?}", e))?;
        self.max_guest_page_table_levels = self.vpercpu.max_guest_page_table_levels();
        self.pa_range = self.vpercpu.pa_range();
        self.pa_bits = self.vpercpu.pa_bits();
        Ok(())
    }

    pub fn max_guest_page_table_levels(&self) -> usize {
        self.max_guest_page_table_levels
    }
}

impl HCpuOp for HCpu {
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

impl riscv_vcpu::CpuHal for VCpuHal {
    fn irq_hanlder(&self) {
        axhal::irq::irq_handler(0);
    }

    fn inject_interrupt(&self, irq: usize) {
        todo!()
    }
}

pub struct VCpu {
    pub vcpu: RISCVVCpu,
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

        let vcpu = RISCVVCpu::new(RISCVVCpuCreateConfig {
            hart_id: hard_id.raw(),
            dtb_addr: dtb_addr.as_usize(),
        })
        .unwrap();
        Ok(VCpu { vcpu, common })
    }

    pub fn set_pt_level(&mut self, level: usize) {
        self.vcpu.pt_level = level;
    }

    pub fn set_pa_bits(&mut self, pa_bits: usize) {
        self.vcpu.pa_bits = pa_bits;
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

        self.vcpu
            .setup()
            .map_err(|e| anyhow!("{e}"))?;
        self.vcpu
            .setup_current_cpu(self.vm_id().into())
            .map_err(|e| anyhow!("{e}"))?;

        while self.is_active() {
            // Forward UART input to guest before VM entry.
            // The UART IRQ may be handled by a different CPU (e.g., CPU 0),
            // which sets UART_INPUT_PENDING. Checking here ensures data is
            // pushed to VirtIO queue before we inject the interrupt below.
            #[cfg(feature = "virtio-console")]
            if axplat_riscv64_qemu_virt::take_uart_input_pending() {
                let forwarded = axdevice::forward_console_input();
                if forwarded > 0 {
                    trace!("Console: forwarded {} bytes to guest", forwarded);
                }
            }

            // Before VM entry: inject any pending interrupts to the guest.
            self.vm()?.with_machine_running(|running| {
                let vmspace = running.vmspace();

                if let Some(pending_irq) = vmspace.pop_pending_interrupt(self.bind_id().raw()) {
                    trace!(
                        "[vCPU] Injecting interrupt via vPLIC: cpu={}, irq={}, device={:?}",
                        self.bind_id(),
                        pending_irq.irq,
                        pending_irq.device_id
                    );
                    vmspace.inject_virtual_interrupt(pending_irq.irq);
                }
            })?;

            // debug!("RISCV vCPU {} entering guest", self.bind_id());
            let exit_reason = self.vcpu.run().map_err(|e| anyhow!("{e}"))?;
            // debug!(
            //     "vCPU {} exited with reason: {:?}",
            //     self.bind_id(),
            //     exit_reason
            // );
            match exit_reason {
                riscv_vcpu::AxVCpuExitReason::Hypercall { nr, args } => todo!(),
                riscv_vcpu::AxVCpuExitReason::MmioRead {
                    addr,
                    width,
                    reg,
                    reg_width,
                    signed_ext,
                } => {
                    // width 已经是 AccessWidth 枚举，直接使用
                    let access_width = width;

                    // 调用设备处理
                    let value = self.vm()?.with_machine_running(|running| {
                        running.vmspace().handle_mmio_read(addr.as_usize().into(), access_width)
                            .map_err(|e| RunError::ExitWithError(anyhow!("MMIO read failed: {e:?}")))
                    })??;

                    // 写回寄存器
                    let final_value = if signed_ext {
                        // 符号扩展
                        match access_width {
                            axaddrspace::device::AccessWidth::Byte => (value as i8) as i64 as u64,
                            axaddrspace::device::AccessWidth::Word => (value as i16) as i64 as u64,
                            axaddrspace::device::AccessWidth::Dword => (value as i32) as i64 as u64,
                            axaddrspace::device::AccessWidth::Qword => value as u64,
                        }
                    } else {
                        value as u64
                    };

                    self.vcpu.set_gpr(reg, final_value as usize);
                    // info!(
                    //     "vCPU {} MMIO read: addr={:#x}, width={:?}, reg={}, value={:#x}",
                    //     self.bind_id(),
                    //     addr,
                    //     access_width,
                    //     reg,
                    //     final_value
                    // );
                }
                riscv_vcpu::AxVCpuExitReason::MmioWrite { addr, width, data } => {
                    // width 已经是 AccessWidth 枚举，直接使用
                    let access_width = width;

                    self.vm()?.with_machine_running(|running| {
                        running.vmspace().handle_mmio_write(addr.as_usize().into(), access_width, data as usize)
                            .map_err(|e| RunError::ExitWithError(anyhow!("MMIO write failed: {e:?}")))
                    })??;

                    // info!(
                    //     "vCPU {} MMIO write: addr={:#x}, width={:?}, data={:#x}",
                    //     self.bind_id(),
                    //     addr,
                    //     access_width,
                    //     data
                    // );
                }
                riscv_vcpu::AxVCpuExitReason::ExternalInterrupt { vector } => {
                    // Handle the external interrupt (PLIC claim happens inside irq_handler)
                    axhal::irq::irq_handler(vector as usize);

                    // Get the IRQ number that was claimed from PLIC
                    if let Some(irq) = axplat_riscv64_qemu_virt::take_last_external_irq() {
                        // For passthrough devices (e.g., UART), inject the interrupt
                        // into the DeviceInterruptManager queue.
                        // It will be delivered to guest via vPLIC when vCPU runs next.
                        let cpu_id = self.bind_id().raw();
                        if let Err(e) = self.vm()?.with_machine_running(|running| {
                            running.vmspace().inject_passthrough_interrupt(irq, cpu_id)
                        })? {
                            warn!(
                                "Failed to inject passthrough interrupt {} to CPU {}: {:?}",
                                irq, cpu_id, e
                            );
                        }
                    }

                }
                riscv_vcpu::AxVCpuExitReason::CpuUp {
                    target_cpu,
                    entry_point,
                    arg,
                } => {
                    debug!("vCPU {} requested CPU {} up", self.bind_id(), target_cpu);
                    self.vm()?.with_machine_running_mut(|running| {
                        debug!("vCPU {} is bringing up CPU {}", self.bind_id(), target_cpu);
                        running.cpu_up(
                            CpuHardId::new(target_cpu as _),
                            GuestPhysAddr::from(entry_point.as_usize()),
                            arg,
                        )
                    })??;
                    self.vcpu.set_gpr(0, 0);
                }
                riscv_vcpu::AxVCpuExitReason::CpuDown { _state } => todo!(),
                riscv_vcpu::AxVCpuExitReason::SystemDown => {
                    info!("vCPU {} requested system shutdown", self.bind_id());
                    self.vm()?.stop()?;
                }
                riscv_vcpu::AxVCpuExitReason::Nothing => {}
                riscv_vcpu::AxVCpuExitReason::Halt => {
                    info!("vCPU {} halted", self.bind_id());
                    break;
                }
                riscv_vcpu::AxVCpuExitReason::NestedPageFault { addr, access_flags } => {
                    error!(
                        "Nested page fault at addr={:#x}, access_flags={:?}, injecting exception to guest",
                        addr, access_flags
                    );
                    // Inject an exception to the guest - this is typically a page fault
                    // For now, we'll need to implement proper exception injection
                    todo!("NestedPageFault: inject exception to guest at addr={:#x}", addr);
                }
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

impl Debug for VCpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VCpu")
            .field("bind_id", &self.bind_id())
            .field("hard_id", &self.hard_id())
            .field("vcpu", &self.vcpu)
            .finish()
    }
}
