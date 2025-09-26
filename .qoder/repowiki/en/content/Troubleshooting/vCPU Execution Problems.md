# vCPU Execution Problems

<cite>
**Referenced Files in This Document**   
- [vcpu.rs](file://src/vcpu.rs)
- [vm.rs](file://src/vm.rs)
- [hal.rs](file://src/hal.rs)
</cite>

## Table of Contents
1. [Introduction](#introduction)
2. [Project Structure](#project-structure)
3. [Core Components](#core-components)
4. [Architecture Overview](#architecture-overview)
5. [Detailed Component Analysis](#detailed-component-analysis)
6. [Dependency Analysis](#dependency-analysis)
7. [Performance Considerations](#performance-considerations)
8. [Troubleshooting Guide](#troubleshooting-guide)
9. [Conclusion](#conclusion)

## Introduction
This document provides comprehensive guidance for diagnosing and resolving vCPU-related issues in the axvm hypervisor framework. It focuses on interpreting vCPU exit reason codes, analyzing execution flow through `run_vcpu()`, identifying stuck exit handlers, and verifying correct vCPU state restoration. The analysis covers architecture-specific implementations for x86_64, riscv64, and aarch64 targets, with emphasis on debugging techniques such as logging exit reasons, validating register states, and checking interrupt injection paths.

## Project Structure

```mermaid
graph TD
A[src/] --> B[config.rs]
A --> C[hal.rs]
A --> D[lib.rs]
A --> E[vcpu.rs]
A --> F[vm.rs]
G[Cargo.toml]
H[README.md]
```

**Diagram sources**
- [vcpu.rs](file://src/vcpu.rs)
- [vm.rs](file://src/vm.rs)

**Section sources**
- [vcpu.rs](file://src/vcpu.rs)
- [vm.rs](file://src/vm.rs)

## Core Components

The core components for vCPU execution management include the architecture-dependent vCPU implementations defined in `vcpu.rs` and the VM lifecycle management in `vm.rs`. The system uses conditional compilation to select appropriate vCPU implementations based on target architecture (x86_64, riscv64, or aarch64). Each vCPU implementation provides architecture-specific configuration types and setup parameters that are crucial for proper initialization and execution.

**Section sources**
- [vcpu.rs](file://src/vcpu.rs#L0-L29)
- [vm.rs](file://src/vm.rs#L69-L106)

## Architecture Overview

```mermaid
graph TD
subgraph "VM Management"
VM[AxVM] --> VCPU[vcpu.rs]
VM --> HAL[hal.rs]
end
subgraph "Architecture Specific"
X86[x86_vcpu] --> VCPU
RISCV[riscv_vcpu] --> VCPU
AARCH64[arm_vcpu] --> VCPU
end
VCPU --> ExitHandler[Exit Reason Handling]
ExitHandler --> MMIO[MMIO Operations]
ExitHandler --> IO[Port I/O]
ExitHandler --> SysReg[System Register Access]
ExitHandler --> PageFault[Nested Page Faults]
VM --> Interrupt[Interrupt Injection]
Interrupt --> HAL
```

**Diagram sources**
- [vcpu.rs](file://src/vcpu.rs#L0-L29)
- [vm.rs](file://src/vm.rs#L403-L487)

## Detailed Component Analysis

### vCPU Implementation Analysis

The vCPU implementation uses conditional compilation to provide architecture-specific types and configurations. For x86_64 targets, it uses VmxArchVCpu with empty configuration, while riscv64 and aarch64 targets have more complex configuration requirements including hart_id/mpidr_el1 and DTB addresses.

```mermaid
classDiagram
class AxArchVCpuImpl {
<<type alias>>
}
class AxVCpuCreateConfig {
<<type alias>>
}
class has_hardware_support {
<<function>>
}
AxArchVCpuImpl <|-- VmxArchVCpu : "x86_64"
AxArchVCpuImpl <|-- RISCVVCpu : "riscv64"
AxArchVCpuImpl <|-- Aarch64VCpu : "aarch64"
AxVCpuCreateConfig <|-- Unit : "x86_64"
AxVCpuCreateConfig <|-- RISCVVCpuCreateConfig : "riscv64"
AxVCpuCreateConfig <|-- Aarch64VCpuCreateConfig : "aarch64"
```

**Diagram sources**
- [vcpu.rs](file://src/vcpu.rs#L0-L29)

**Section sources**
- [vcpu.rs](file://src/vcpu.rs#L0-L29)

### VM Execution Flow Analysis

The `run_vcpu()` function implements the main execution loop for virtual CPUs, handling various exit reasons and dispatching them to appropriate handlers. The function follows a structured pattern of binding the vCPU to a physical CPU, entering an execution loop, processing exit reasons, and unbinding upon completion.

```mermaid
sequenceDiagram
participant VM as AxVM
participant VCpu as AxVCpu
participant Devices as AxVmDevices
VM->>VCpu : bind()
loop Execution Loop
VM->>VCpu : run()
VCpu-->>VM : AxVCpuExitReason
alt Handled Exit
VM->>VM : Match exit reason
VM->>Devices : Handle device operation
VM->>VCpu : Set GPR/Register
VM->>VM : Continue loop
else Unhandled Exit
VM->>VCpu : break loop
end
end
VM->>VCpu : unbind()
VM-->>Caller : Return exit reason
```

**Diagram sources**
- [vm.rs](file://src/vm.rs#L403-L487)

**Section sources**
- [vm.rs](file://src/vm.rs#L403-L487)

### Exit Reason Handling Analysis

The system handles multiple types of vCPU exit reasons, each requiring specific processing logic. The handler loop processes MMIO operations, port I/O, system register access, and nested page faults, while passing through unhandled exits to the caller.

```mermaid
flowchart TD
Start([Start run_vcpu]) --> Bind["vcpu.bind()"]
Bind --> RunLoop["Enter execution loop"]
RunLoop --> Execute["vcpu.run()"]
Execute --> CheckExit{"Exit handled?"}
CheckExit --> |Yes| MMIORead["AxVCpuExitReason::MmioRead"]
CheckExit --> |Yes| MMIOWrite["AxVCpuExitReason::MmioWrite"]
CheckExit --> |Yes| IORead["AxVCpuExitReason::IoRead"]
CheckExit --> |Yes| IOWrite["AxVCpuExitReason::IoWrite"]
CheckExit --> |Yes| SysRegRead["AxVCpuExitReason::SysRegRead"]
CheckExit --> |Yes| SysRegWrite["AxVCpuExitReason::SysRegWrite"]
CheckExit --> |Yes| PageFault["AxVCpuExitReason::NestedPageFault"]
CheckExit --> |No| BreakLoop["Break loop with exit reason"]
MMIORead --> Devices["get_devices().handle_mmio_read()"]
Devices --> SetGPR["vcpu.set_gpr(reg, val)"]
SetGPR --> ContinueLoop["Continue loop"]
MMIOWrite --> DevicesWrite["get_devices().handle_mmio_write()"]
DevicesWrite --> ContinueLoop
IORead --> PortRead["get_devices().handle_port_read()"]
PortRead --> SetEAX["vcpu.set_gpr(0, val)"]
SetEAX --> ContinueLoop
IOWrite --> PortWrite["get_devices().handle_port_write()"]
PortWrite --> ContinueLoop
SysRegRead --> SysRegReadOp["get_devices().handle_sys_reg_read()"]
SysRegReadOp --> SetReg["vcpu.set_gpr(reg, val)"]
SetReg --> ContinueLoop
SysRegWrite --> SysRegWriteOp["get_devices().handle_sys_reg_write()"]
SysRegWriteOp --> ContinueLoop
PageFault --> AddressSpace["address_space.handle_page_fault()"]
PageFault --> ContinueLoop
ContinueLoop --> Execute
BreakLoop --> Unbind["vcpu.unbind()"]
Unbind --> Return["Return exit reason"]
Return --> End([End])
```

**Diagram sources**
- [vm.rs](file://src/vm.rs#L424-L487)

**Section sources**
- [vm.rs](file://src/vm.rs#L424-L487)

## Dependency Analysis

```mermaid
graph LR
vm_rs --> vcpu_rs
vm_rs --> hal_rs
vm_rs --> axvcpu_crate
vm_rs --> axdevice_crate
vm_rs --> axaddrspace_crate
vcpu_rs --> x86_vcpu_crate
vcpu_rs --> riscv_vcpu_crate
vcpu_rs --> arm_vcpu_crate
vcpu_rs --> arm_vgic_crate
hal_rs --> axaddrspace_crate
hal_rs --> axvcpu_crate
```

**Diagram sources**
- [vm.rs](file://src/vm.rs#L13)
- [vcpu.rs](file://src/vcpu.rs#L0-L29)
- [hal.rs](file://src/hal.rs#L0-L43)

**Section sources**
- [vm.rs](file://src/vm.rs#L13)
- [vcpu.rs](file://src/vcpu.rs#L0-L29)
- [hal.rs](file://src/hal.rs#L0-L43)

## Performance Considerations
The vCPU execution model employs a tight loop for handling exits, which can impact performance if exit handlers are not optimized. Frequent MMIO operations or unhandled exits can lead to increased overhead. The use of tracing macros (`trace!("{exit_reason:#x?}")`) provides visibility into exit patterns but should be disabled in production builds to minimize performance impact.

## Troubleshooting Guide

When diagnosing vCPU-related issues, focus on the following areas:

1. **Infinite exit loops**: Check if exit handlers are properly marking exits as handled. Ensure MMIO operations are being processed correctly.
2. **Failed vCPU binding**: Verify physical CPU affinity settings in the VM configuration and ensure the HAL implementation correctly handles vCPU to pCPU mapping.
3. **Unexpected termination**: Examine unhandled exit reasons and ensure all required device emulation is available.
4. **Timer interrupt issues**: For aarch64 targets, verify virtual timer setup in non-passthrough mode and check sysreg device registration.
5. **Inter-processor communication**: Validate interrupt injection paths through the HAL's `inject_irq_to_vcpu` method.

Common mistakes in HAL implementation include incorrect physical address translation, improper current VM ID reporting, and faulty interrupt injection logic.

**Section sources**
- [vm.rs](file://src/vm.rs#L489-L538)
- [hal.rs](file://src/hal.rs#L0-L43)

## Conclusion
Effective diagnosis of vCPU execution problems requires understanding the interaction between architecture-specific implementations, the VM execution loop, and the HAL interface. By analyzing exit reason codes, verifying proper handler implementation, and ensuring correct state restoration, most vCPU-related issues can be systematically resolved. The use of conditional compilation guards allows for targeted debugging of architecture-specific bugs in x86_64, riscv64, and aarch64 targets.