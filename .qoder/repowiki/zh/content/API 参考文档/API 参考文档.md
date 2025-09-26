
# API 参考文档

<cite>
**本文档中引用的文件**  
- [lib.rs](file://src/lib.rs)
- [config.rs](file://src/config.rs)
- [hal.rs](file://src/hal.rs)
- [vm.rs](file://src/vm.rs)
</cite>

## 目录
1. [AxVM 结构体方法参考](#axvm-结构体方法参考)  
2. [AxVMConfig 配置结构详解](#axvmconfig-配置结构详解)  
3. [AxVMHal 特征接口说明](#axvmhal-特征接口说明)

## AxVM 结构体方法参考

`AxVM<H: AxVMHal, U: AxVCpuHal>` 是表示虚拟机的核心结构，封装了 VM 的运行状态、vCPU 列表、地址空间和设备等信息。以下为其实现的关键公共方法。

### new
创建一个具有指定配置的新虚拟机实例。

- **函数签名**: `pub fn new(config: AxVMConfig) -> AxResult<AxVMRef<H, U>>`
- **参数含义**:
  - `config`: 虚拟机的配置对象，类型为 `AxVMConfig`。
- **返回类型**: `AxResult<AxVMRef<H, U>>`，成功时返回包装在 `Arc` 中的 `AxVM` 实例。
- **可能的错误码**:
  - `InvalidInput`: 内存区域标志非法或配置无效。
  - `Unsupported`: 硬件不支持虚拟化（通过 `has_hardware_support()` 检测）。
  - 其他来自底层地址空间或设备初始化的错误。
- **调用上下文说明**: 此方法应在 VM 启动前调用，用于根据配置构建 VM 实例。它会完成 vCPU 创建、内存映射和设备初始化。
- **典型使用模式**: 在系统启动阶段，从 TOML 配置文件解析出 `AxVMCrateConfig`，转换为 `AxVMConfig` 后传入此方法。
- **性能注意事项**: 初始化过程涉及内存分配和页表设置，应尽量避免频繁创建/销毁 VM。
- **调用示例**:
```rust
let vm_config = AxVMConfig::from(toml_config);
let vm = AxVM::new(vm_config)?;
```

[SPEC SYMBOL](file://src/vm.rs#L75-L281)

### boot
启动虚拟机。

- **函数签名**: `pub fn boot(&self) -> AxResult`
- **参数含义**: 无。
- **返回类型**: `AxResult`，成功时返回 `Ok(())`。
- **可能的错误码**:
  - `Unsupported`: 硬件不支持虚拟化。
  - `BadState`: VM 已经处于运行状态。
- **调用上下文说明**: 必须在 `new` 成功后调用。此方法将内部的 `running` 标志设为 `true`，允许 vCPU 运行。
- **典型使用模式**: 在 VM 完成所有配置和初始化后，调用 `boot` 来启动其执行。
- **性能注意事项**: 该操作本身开销极小，仅为原子写操作。
- **调用示例**:
```rust
vm.boot()?;
```

[SPEC SYMBOL](file://src/vm.rs#L364-L379)

### shutdown
关闭虚拟机。

- **函数签名**: `pub fn shutdown(&self) -> AxResult`
- **参数含义**: 无。
- **返回类型**: `AxResult`，成功时返回 `Ok(())`。
- **可能的错误码**:
  - `BadState`: VM 已经处于关闭过程中。
- **调用上下文说明**: 将内部的 `shutting_down` 标志设为 `true`。目前设计为一次性操作，VM 关闭后无法重新启动。
- **典型使用模式**: 在需要终止 VM 执行时调用。
- **性能注意事项**: 该操作本身开销极小，仅为原子写操作。
- **调用示例**:
```rust
vm.shutdown()?;
```

[SPEC SYMBOL](file://src/vm.rs#L387-L404)

### run_vcpu
运行指定 ID 的 vCPU。

- **函数签名**: `pub fn run_vcpu(&self, vcpu_id: usize) -> AxResult<AxVCpuExitReason>`
- **参数含义**:
  - `vcpu_id`: 要运行的 vCPU 的 ID。
- **返回类型**: `AxResult<AxVCpuExitReason>`，成功时返回 vCPU 退出的原因。
- **可能的错误码**:
  - `InvalidInput`: 提供的 `vcpu_id` 无效（超出范围）。
  - 来自 `vcpu.run()` 或 I/O 处理的其他错误。
- **调用上下文说明**: 此方法是 vCPU 执行循环的核心。它会绑定到当前物理 CPU，然后进入一个循环，处理 vCPU 退出事件（如 MMIO、I/O、中断等），直到遇到未被处理的退出原因。
- **典型使用模式**: 在宿主操作系统调度器中，为每个物理 CPU 分配一个线程来轮询并运行其关联的 vCPU。
- **性能注意事项**: 循环内对 MMIO 和 I/O 的模拟是主要开销来源，优化这些处理程序至关重要。
- **调用示例**:
```rust
let exit_reason = vm.run_vcpu(0)?;
match exit_reason {
    AxVCpuExitReason::Hlt => { /* Handle HLT */ },
    _ => { /* Handle other reasons */ }
}
```

[SPEC SYMBOL](file://src/vm.rs#L406-L478)

### map_region
将主机物理内存区域映射到客户机物理内存。

- **函数签名**: `pub fn map_region(&self, gpa: GuestPhysAddr, hpa: HostPhysAddr, size: usize, flags: MappingFlags) -> AxResult<()>`
- **参数含义**:
  - `gpa`: 客户机物理地址。
  - `hpa`: 主机物理地址。
  - `size`: 映射区域大小（字节）。
  - `flags`: 内存映射标志（读、写、执行、设备等）。
- **返回类型**: `AxResult<()>`。
- **可能的错误码**: 来自底层地址空间管理器的错误，例如映射失败。
- **调用上下文说明**: 用于动态地为客户机添加内存映射，例如热插拔内存或共享内存区域。
- **典型使用模式**: 在 VM 运行时，需要为客户机提供额外的物理内存访问权限时。
- **性能注意事项**: 触发页表更新，可能导致 TLB 刷新。
- **调用示例**:
```rust
vm.map_region(gpa, hpa, size, MappingFlags::READ | MappingFlags::WRITE)?;
```

[SPEC SYMBOL](file://src/vm.rs#L520-L525)

### unmap_region
取消映射客户机物理内存区域。

- **函数签名**: `pub fn unmap_region(&self, gpa: GuestPhysAddr, size: usize) -> AxResult<()>`
- **参数含义**:
  - `gpa`: 要取消映射的客户机物理地址。
  - `size`: 区域大小。
- **返回类型**: `AxResult<()>`。
- **可能的错误码**: 来自底层地址空间管理器的错误，例如未找到映射。
- **调用上下文说明**: 与 `map_region` 对应，用于移除现有的内存映射。
- **典型使用模式**: 动态移除不再需要的内存区域。
- **性能注意事项**: 触发页表更新，可能导致 TLB 刷新。
- **调用示例**:
```rust
vm.unmap_region(gpa, size)?;
```

[SPEC SYMBOL](file://src/vm.rs#L527-L531)

### inject_interrupt_to_vcpu
向指定的 vCPU 注入中断。

- **函数签名**: `pub fn inject_interrupt_to_vcpu(&self, targets: CpuMask<TEMP_MAX_VCPU_NUM>, irq: usize) -> AxResult`
- **参数含义**:
  - `targets`: 目标 vCPU 的掩码。
  - `irq`: 要注入的中断号。
- **返回类型**: `AxResult`。
- **可能的错误码**:
  - `Panic`: 如果尝试向另一个 VM 的 vCPU 注入中断（当前不支持跨 VM 中断注入）。
  - 来自 `H::inject_irq_to_vcpu` 的错误。
- **调用上下文说明**: 用于软件触发中断，例如定时器中断或 IPI（处理器间中断）。它依赖于 `AxVMHal` 的实现来定位目标 vCPU 并注入中断。
- **典型使用模式**: 在虚拟设备驱动或 VMM 内部逻辑中，当需要通知客户机 OS 时调用。
- **性能注意事项**: 涉及查找目标 vCPU 所在的物理 CPU，有一定开销。
- **调用示例**:
```rust
let mut mask = CpuMask::new();
mask.insert(0); // Target vCPU 0
vm.inject_interrupt_to_vcpu(mask, 32)?; // Inject IRQ 32
```

[SPEC SYMBOL](file://src/vm.rs#L480-L499)

**Section sources**
- [vm.rs](file://src/vm.rs#L75-L531)

## AxVMConfig 配置结构详解

`AxVMConfig` 结构体定义了创建虚拟机所需的所有配置参数。它通常由更高层的 `AxVMCrateConfig`（来自 TOML 文件）转换而来。

### 字段说明
- `id`: 虚拟机的唯一标识符。
- `name`: 虚拟机的人类可读名称。
- `vm_type`: 虚拟机类型（如通用型、实时型）。
- `cpu_num`: 虚拟 CPU (vCPU) 的数量。
- `phys_cpu_ids`: 可选的 vCPU ID 到物理 CPU ID 的映射。
- `phys_cpu_sets`: 可选的每个 vCPU 的 CPU 亲和性掩码。
- `cpu_config`: 包含 BSP 和 AP 入口点的 `AxVCpuConfig`。
- `image_config`: 包含内核、BIOS、DTB、ramdisk 加载地址的 `VMImageConfig`。
- `memory_regions`: 内存区域列表 (`Vec<VmMemConfig>`)。
- `emu_devices`: 模拟设备配置列表 (`Vec<EmulatedDeviceConfig>`)。
- `pass_through_devices`: 直通设备配置列表 (`Vec<PassThroughDeviceConfig>`)。
- `spi_list`: 在直通模式下要分配的 SPI 中断列表。
- `interrupt_mode`: 中断交付模式（直通或虚拟化）。

### 构造方法
`AxVMConfig` 主要通过 `From<AxVMCrateConfig>` trait 从外部配置构造。

- **函数签名**: `impl From<AxVMCrateConfig> for AxVMConfig`
- **作用**: 将来自 TOML 文件的 `AxVMCrateConfig` 转换为运行时使用的 `AxVMConfig`。
- **关键转换步骤**:
  1. 基本属性（ID、名称）直接复制。
  2. VM 类型字符串转换为 `VMType` 枚举。
  3. 设置 BSP 和 AP 的入口点。
  4. 转换内核镜像加载地址。
  5. 转移设备和内存区域配置。

[SPEC SYMBOL](file://src/config.rs#L66-L103)

### 方法说明
- `id()`: 返回 VM 的 ID。
- `name()`: 返回 VM 的名称副本。
- `get_vcpu_affinities_pcpu_ids()`: 生成 `(vcpu_id, pcpu_affinity_mask, physical_id)` 元组列表，用于调度决策。
- `image_config()`: 返回对镜像加载配置的引用。
- `bsp_entry()`: 返回引导处理器 (BSP) 的入口点（GPA）。
- `ap_entry()`: 返回