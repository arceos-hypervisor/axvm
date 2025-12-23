use core::ops::Deref;

use alloc::vec::Vec;

use crate::{
    AxVMConfig, GuestPhysAddr, VmAddrSpace, VmMachineUninitOps,
    arch::{VmMachineInited, cpu::VCpu},
    config::CpuNumType,
    data::VmDataWeak,
};

pub struct VmMachineUninit {
    config: AxVMConfig,
    pt_levels: usize,
    pa_max: usize,
    pa_bits: usize,
}

impl VmMachineUninitOps for VmMachineUninit {
    type Inited = VmMachineInited;

    fn new(config: AxVMConfig) -> Self {
        Self {
            config,
            pt_levels: 4, // x86_64 使用 4 级页表 (PML4)
            pa_max: usize::MAX,
            pa_bits: 48, // 典型的 x86_64 物理地址宽度
        }
    }

    fn init(mut self, vmdata: VmDataWeak) -> Result<Self::Inited, anyhow::Error>
    where
        Self: Sized,
    {
        self.init_raw(vmdata)
    }
}

impl VmMachineUninit {
    fn new_vcpus(&mut self, vm: &VmDataWeak) -> anyhow::Result<Vec<VCpu>> {
        // 创建 vCPUs
        let mut vcpus = vec![];

        // x86 不使用设备树，dtb_addr 参数设为 0
        let dtb_addr = GuestPhysAddr::from(0);

        match self.config.cpu_num {
            CpuNumType::Alloc(num) => {
                for _ in 0..num {
                    let vcpu = VCpu::new(None, dtb_addr, vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
            CpuNumType::Fixed(ref ids) => {
                for id in ids {
                    let vcpu = VCpu::new(Some(*id), dtb_addr, vm.clone())?;
                    debug!("Created vCPU with {:?}", vcpu.bind_id());
                    vcpus.push(vcpu);
                }
            }
        }

        let vcpu_count = vcpus.len();

        // x86_64 平台的固定配置
        // 从 HCpu 获取页表级别和地址位信息（如果需要）
        for vcpu in &vcpus {
            // x86_64 固定使用 4 级页表
            // PA bits 可以根据需要调整
            debug!("vCPU bind_id: {:?}", vcpu.bind_id());
        }

        // 如果 pt_levels == 3，需要限制 pa_max
        if self.pt_levels == 3 {
            self.pa_max = self.pa_max.min(0x8000000000);
        }

        debug!(
            "VM {} ({}) vCPU count: {}, \n  Max Guest Page Table Levels: {}\n  Max PA: {:#x}\n  PA Bits: {}",
            self.config.id, self.config.name, vcpu_count, self.pt_levels, self.pa_max, self.pa_bits
        );
        Ok(vcpus)
    }

    fn init_raw(&mut self, vmdata: VmDataWeak) -> anyhow::Result<VmMachineInited> {
        debug!("Initializing VM {} ({})", self.config.id, self.config.name);
        let mut cpus = self.new_vcpus(&vmdata)?;

        let mut vmspace =
            VmAddrSpace::new(self.pt_levels, GuestPhysAddr::from(0)..self.pa_max.into())?;

        debug!(
            "Mapping memory regions for VM {} ({})",
            self.config.id, self.config.name
        );
        for memory_cfg in &self.config.memory_regions {
            vmspace.new_memory(memory_cfg)?;
        }

        vmspace.load_kernel_image(&self.config)?;

        // x86 不使用设备树，而是使用 ACPI 表
        // 这里我们跳过 FDT 创建，直接加载内核
        // 如果需要 ACPI，可以在后续添加

        vmspace.map_passthrough_regions()?;

        let kernel_entry = vmspace.kernel_entry();
        let gpt_root = vmspace.gpt_root();

        // 设置 vCPUs
        for vcpu in &mut cpus {
            vcpu.vcpu
                .set_entry(kernel_entry.as_usize().into())
                .map_err(|e| anyhow::anyhow!("Failed to set entry: {:?}", e))?;

            vcpu.vcpu
                .set_ept_root(gpt_root)
                .map_err(|e| anyhow::anyhow!("Failed to set EPT root: {:?}", e))?;

            // x86 特定的 VCPU 设置
            // 注意：x86_vcpu 的 VmxVcpu 不需要额外的 setup 调用
            // 因为在创建时已经完成基本初始化
        }

        Ok(VmMachineInited {
            id: self.config.id.into(),
            name: self.config.name.clone(),
            vmspace,
            vcpus: cpus,
        })
    }
}
