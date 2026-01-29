use alloc::vec::Vec;

use crate::vhal::{
    ArchHal,
    cpu::{CpuHardId, CpuId},
};
use memory_addr::VirtAddr;

use super::cpu::HCpu;

// 使用 x86_vcpus 提供的 raw_cpuid
extern crate raw_cpuid;

pub struct Hal;

impl ArchHal for Hal {
    fn init() -> anyhow::Result<()> {
        // x86_vcpu 不需要全局初始化
        // 每个独立的 CPU 在 current_cpu_init 中单独初始化 VMX
        info!("x86_64 HAL initialization complete (no global init required)");
        Ok(())
    }

    fn current_cpu_init(id: CpuId) -> anyhow::Result<HCpu> {
        info!("Enabling virtualization on x86_64 cpu {}", id);
        let mut cpu = HCpu::new(id);
        cpu.init()?;
        info!("{}", cpu);
        Ok(cpu)
    }

    fn cpu_list() -> Vec<CpuHardId> {
        // 简单实现：从 axruntime 获取 CPU 数量
        // 假设 CPU ID 连续（0, 1, 2, ...）
        // TODO: 后续可以从 ACPI/MP 表获取更准确的 APIC ID 映射
        let count = axruntime::cpu_count();
        debug!("x86_64 CPU list: {} CPUs (simple implementation)", count);
        (0..count).map(|i| CpuHardId::new(i)).collect()
    }

    fn cpu_hard_id() -> CpuHardId {
        // 使用 raw_cpuid 获取当前 CPU 的 APIC ID
        let apic_id = raw_cpuid::CpuId::new()
            .get_feature_info()
            .map(|f| f.initial_local_apic_id() as usize)
            .unwrap_or_else(|| {
                warn!("Failed to get APIC ID from CPUID, using fallback");
                0
            });
        CpuHardId::new(apic_id)
    }

    fn cache_flush(_vaddr: VirtAddr, _size: usize) {
        // x86 不需要显式的缓存刷新
        // WBINVD 指令会在需要时由硬件自动处理
    }
}
