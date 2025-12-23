use core::{
    fmt::{self, Debug, Display},
    ops::Deref,
};
use std::os::arceos::modules::axalloc;

use axvm_types::addr::*;
use memory_addr::{PAGE_SIZE_4K, PhysAddr, VirtAddr};

use crate::{
    RunError,
    data::VmDataWeak,
    vcpu::{VCpuCommon, VCpuOp},
    vhal::{
        ArchCpuData,
        cpu::{CpuHardId, CpuId},
    },
};

// ==================== x86 VCPU HAL 实现 ====================
// x86_vcpu 现在使用自己的 Hal trait，不需要 AxVCpuHal

/// x86 VCPU 的 HAL 实现 - 实现 x86_vcpu::Hal trait
pub(super) struct X86VcpuHal;

impl x86_vcpu::Hal for X86VcpuHal {
    fn alloc_frame() -> Option<usize> {
        axalloc::global_allocator()
            .alloc_pages(1, PAGE_SIZE_4K, axalloc::UsageKind::Global)
            .ok()
    }

    fn dealloc_frame(paddr: usize) {
        axalloc::global_allocator().dealloc_pages(paddr, 1, axalloc::UsageKind::Global);
    }

    fn phys_to_virt(paddr: usize) -> usize {
        axhal::mem::phys_to_virt(PhysAddr::from(paddr)).into()
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        axhal::mem::virt_to_phys(VirtAddr::from(vaddr)).into()
    }
}

// 使用具体的泛型类型
type VmxPerCpuState = x86_vcpu::VmxArchPerCpuState<X86VcpuHal>;
type VmxVcpu = x86_vcpu::VmxArchVCpu<X86VcpuHal>;

pub struct HCpu {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    vpercpu: VmxPerCpuState,
    max_guest_page_table_levels: usize,
    pub pa_range: core::ops::Range<usize>,
    pub pa_bits: usize,
}

impl HCpu {
    pub fn new(id: CpuId) -> Self {
        // 使用 raw_cpuid 获取 x86 APIC ID
        let apic_id = raw_cpuid::CpuId::new()
            .get_feature_info()
            .map(|f| f.initial_local_apic_id() as usize)
            .unwrap_or(0);
        let hard_id = CpuHardId::new(apic_id);

        // 创建 x86 PerCpu 状态
        let vpercpu = VmxPerCpuState::new(id.raw()).expect("Failed to create VmxPerCpuState");

        HCpu {
            id,
            hard_id,
            vpercpu,
            max_guest_page_table_levels: 0,
            pa_range: 0..0,
            pa_bits: 0,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        // 启用 VMX 硬件虚拟化
        self.vpercpu.hardware_enable()?;

        // x86_64 平台的固定配置
        self.max_guest_page_table_levels = 4; // 4-level page tables (PML4)
        self.pa_bits = 48; // 典型的 x86_64 物理地址宽度
        self.pa_range = 0..(1 << self.pa_bits);

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
  PT Levels: {}
  PA Bits: {}",
            self.id, self.hard_id, self.max_guest_page_table_levels, self.pa_bits
        )
    }
}

// x86 特定的 VCPU
pub struct VCpu {
    pub vcpu: VmxVcpu,
    common: VCpuCommon,
}

impl VCpu {
    pub fn new(
        host_cpuid: Option<CpuId>,
        _dtb_addr: GuestPhysAddr, // 参数保留以保持接口兼容性，x86 不使用设备树
        vm: VmDataWeak,
    ) -> anyhow::Result<Self> {
        let common = VCpuCommon::new_exclusive(host_cpuid, vm)?;

        let hard_id = common.hard_id();
        let vm_id = common.vm_id().into();
        let vcpu_id = common.bind_id().raw();

        // 使用 x86_vcpu 的新方法创建 VCPU
        let vcpu = VmxVcpu::new(vm_id, vcpu_id)
            .map_err(|e| anyhow::anyhow!("Failed to create VmxVcpu: {:?}", e))?;

        info!(
            "Created x86 VCPU: vm_id={}, vcpu_id={}, hard_id={}",
            vm_id,
            vcpu_id,
            hard_id.raw()
        );

        Ok(VCpu { vcpu, common })
    }

    pub fn set_pt_level(&mut self, level: usize) {
        // x86 通过 EPT 配置，此方法预留以保持接口兼容性
        debug!("Setting page table level to {} (no-op on x86)", level);
    }

    pub fn set_pa_bits(&mut self, pa_bits: usize) {
        // x86 通过 EPT 配置，此方法预留以保持接口兼容性
        debug!("Setting PA bits to {} (no-op on x86)", pa_bits);
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
        info!("Starting x86 vCPU {}", self.bind_id());

        // 绑定到当前 CPU - 使用 x86_vcpu 的新方法
        self.vcpu.bind().map_err(|e| {
            RunError::ExitWithError(anyhow::anyhow!("Failed to bind VCPU: {:?}", e))
        })?;

        while self.is_active() {
            debug!("x86 vCPU {} entering guest", self.bind_id());

            // 使用 x86_vcpu 的 run_arch 方法，返回自己的 VmxExitReason
            let exit_reason = self.vcpu.run().map_err(|e| {
                RunError::ExitWithError(anyhow::anyhow!("VCPU run failed: {:?}", e))
            })?;

            debug!(
                "x86 vCPU {} exited with reason: {:?}",
                self.bind_id(),
                exit_reason
            );

            // 根据用户优先级处理退出原因 - 使用 x86_vcpu 的 VmxExitReason
            match exit_reason {
                // 高优先级：外部中断（必须实现）
                x86_vcpu::VmxExitReason::ExternalInterrupt { vector } => {
                    debug!("Handling external interrupt, vector={}", vector);
                    axhal::irq::irq_handler(vector);
                }

                // 高优先级：系统寄存器访问 (MSR)
                x86_vcpu::VmxExitReason::SysRegRead { addr, reg } => {
                    // TODO: 实现 MSR 读取处理
                    // x86_vcpu 的 VmxVcpu 已经处理了 x2APIC MSR 访问
                    // 这里需要处理其他 MSR 的读取
                    todo!("MSR read: addr={:?}, reg={}", addr, reg);
                }
                x86_vcpu::VmxExitReason::SysRegWrite { addr, value } => {
                    // TODO: 实现 MSR 写入处理
                    todo!("MSR write: addr={:?}, value={:#x}", addr, value);
                }

                // 高优先级：IO 指令
                x86_vcpu::VmxExitReason::IoRead { port, width } => {
                    // TODO: 实现端口 IO 读取
                    // 需要连接到设备模拟层
                    todo!("IO read: port={:?}, width={:?}", port, width);
                }
                x86_vcpu::VmxExitReason::IoWrite { port, width, data } => {
                    // TODO: 实现端口 IO 写入
                    // 需要连接到设备模拟层
                    todo!(
                        "IO write: port={:?}, width={:?}, data={:#x}",
                        port,
                        width,
                        data
                    );
                }

                // 中优先级：超级调用
                x86_vcpu::VmxExitReason::Hypercall { nr, args } => {
                    // TODO: 实现超级调用接口
                    todo!("Hypercall: nr={:#x}, args={:?}", nr, args);
                }

                // 低优先级：CPU 启动
                x86_vcpu::VmxExitReason::CpuUp {
                    target_cpu,
                    entry_point,
                    arg,
                } => {
                    debug!(
                        "x86 vCPU {} requested CPU {} up",
                        self.bind_id(),
                        target_cpu
                    );
                    self.vm()?.with_machine_running_mut(|running| {
                        debug!("vCPU {} is bringing up CPU {}", self.bind_id(), target_cpu);
                        // 将 axaddrspace::GuestPhysAddr 转换为 axvm_types::addr::GuestPhysAddr
                        // 先转为 usize，再转为目标类型
                        let entry: GuestPhysAddr = entry_point.as_usize().into();
                        running.cpu_up(CpuHardId::new(target_cpu), entry, arg)
                    })??;
                    // x86 使用 SIPI (Startup IPI) 启动 AP，返回值在 RAX 中
                    self.vcpu.set_gpr(0, 0);
                }

                x86_vcpu::VmxExitReason::CpuDown { state } => {
                    // TODO: 实现 CPU 关闭
                    todo!("CPU down: state={:?}", state);
                }

                // 系统关闭
                x86_vcpu::VmxExitReason::SystemDown => {
                    info!("x86 vCPU {} requested system shutdown", self.bind_id());
                    self.vm()?.stop()?;
                    break;
                }

                x86_vcpu::VmxExitReason::Nothing => {
                    // 无操作，继续运行
                }

                _ => {
                    warn!("Unhandled x86 VCPU exit reason: {:?}", exit_reason);
                }
            }
        }

        // 解绑 VCPU - 使用 x86_vcpu 的新方法
        self.vcpu.unbind().map_err(|e| {
            RunError::ExitWithError(anyhow::anyhow!("Failed to unbind VCPU: {:?}", e))
        })?;

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
        f.debug_struct("x86::VCpu")
            .field("bind_id", &self.bind_id())
            .field("hard_id", &self.hard_id())
            .field("vcpu", &self.vcpu)
            .finish()
    }
}
