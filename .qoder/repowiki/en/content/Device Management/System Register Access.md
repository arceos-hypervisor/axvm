# System Register Access

<cite>
**Referenced Files in This Document**
- [vm.rs](file://src/vm.rs)
- [vcpu.rs](file://src/vcpu.rs)
</cite>

## Table of Contents
1. [Introduction](#introduction)
2. [System Register Interception Mechanism](#system-register-interception-mechanism)
3. [Handler Dispatch and Emulation](#handler-dispatch-and-emulation)
4. [Emulated vs Pass-Through Registers](#emulated-vs-pass-through-registers)
5. [Correctness Requirements](#correctness-requirements)
6. [Performance Considerations](#performance-considerations)
7. [Debugging Strategies](#debugging-strategies)

## Introduction
AxVM provides a virtualization framework that enables interception and emulation of architectural system registers such as control registers, Model-Specific Registers (MSRs) on x86, and Special Function Registers (SFRs) on RISC-V. Guest code accesses these registers through standard instructions, which are trapped by hardware virtualization extensions (e.g., VMX on x86, VHE on AArch64). AxVM leverages these traps to gain control when guest code attempts to read from or write to system registers, allowing for full virtualization of the register interface.

The hypervisor intercepts register access via vCPU exits and routes them to dedicated handlers based on the register identity and access type (read/write). This mechanism enables both full emulation of virtualized state (such as time or CPU identification values) and selective pass-through to underlying hardware where appropriate.

**Section sources**
- [vm.rs](file://src/vm.rs#L0-L627)

## System Register Interception Mechanism
When guest code performs a system register access, modern processors with virtualization support generate a VM exit, transferring control back to the hypervisor. In AxVM, this is handled within the `run_vcpu` method of the `AxVM` structure, where the vCPU execution loop processes various exit reasons defined in `AxVCpuExitReason`.

System register reads and writes are specifically identified by the `SysRegRead` and `SysRegWrite` variants of `AxVCpuExitReason`. Upon encountering such an exit, AxVM forwards the operation to device-level handlers through the VM's device manager. The dispatch occurs directly in the exit handling loop:

```rust
AxVCpuExitReason::SysRegRead { addr, reg } => {
    let val = self.get_devices().handle_sys_reg_read(*addr, AccessWidth::Qword)?;
    vcpu.set_gpr(*reg, val);
    true
}
AxVCpuExitReason::SysRegWrite { addr, value } => {
    self.get_devices().handle_sys_reg_write(*addr, AccessWidth::Qword, *value as usize)?;
    true
}
```

This interception mechanism ensures that all sensitive or virtualizable register accesses are mediated by the hypervisor before being either emulated or forwarded.

**Section sources**
- [vm.rs](file://src/vm.rs#L459-L470)

## Handler Dispatch and Emulation
AxVM uses a centralized device management system to handle system register operations. The `get_devices()` method returns a reference to the `AxVmDevices` instance associated with the VM, which maintains a collection of registered system register devices.

For architectures like AArch64, system register devices are explicitly added during VM creation when not operating in passthrough mode:
```rust
for dev in get_sysreg_device() {
    devices.add_sys_reg_dev(dev);
}
```
This registration pattern allows modular addition of system register handlers, enabling extensible support for different classes of registers (e.g., timer, identification, power management).

Each registered device implements the necessary logic to handle read and write operations on specific register addresses. The dispatch table structure is implicitly maintained by the device container, which routes incoming register access requests to the appropriate handler based on the register address (`addr` field in exit reason).

**Section sources**
- [vm.rs](file://src/vm.rs#L254-L283)
- [vcpu.rs](file://src/vcpu.rs#L0-L29)

## Emulated vs Pass-Through Registers
AxVM distinguishes between fully emulated registers and those allowed to pass through with optional filtering:

- **Fully Emulated Registers**: These include registers whose values must be virtualized, such as CPU identification registers (e.g., `CPUID`, `MPIDR_EL1`) or time-related registers (e.g., `CNTVCT_EL0`). For these, AxVM synthesizes appropriate virtual values that maintain consistency across vCPUs while hiding physical topology.
  
- **Pass-Through Registers**: Some registers may be safely exposed directly to the guest, particularly when running in less restrictive configurations. In such cases, AxVM can allow direct access after optional validation or masking of certain bits to enforce security policies.

The decision between emulation and pass-through is determined at VM configuration time and implemented through the presence or absence of corresponding handler devices in the `AxVmDevices` collection.

**Section sources**
- [vm.rs](file://src/vm.rs#L226-L252)

## Correctness Requirements
To ensure proper virtualization semantics, AxVM enforces several correctness requirements during system register handling:

- **Bit Field Preservation**: When emulating registers, individual bit fields must be preserved according to architecture specifications. Reserved or hardwired bits must return architecturally mandated values.
  
- **Guest Restrictions Enforcement**: Sensitive operations (e.g., disabling interrupts, modifying page table controls) must be validated against guest privilege levels and configuration policies.

- **vCPU Consistency**: Virtualized register values (such as time counters or CPU topology information) must remain consistent across all vCPUs within the same VM to prevent guest OS confusion or malfunction.

These constraints are enforced within the respective system register handler implementations registered with the device subsystem.

**Section sources**
- [vm.rs](file://src/vm.rs#L452-L487)

## Performance Considerations
Frequent system register accesses—particularly timekeeping registers like `TSC` on x86 or `CNTVCT_EL0` on AArch64—can significantly impact performance due to the overhead of VM exits and handler invocation. To mitigate this:

- **Caching**: Frequently accessed read-only or slowly changing registers can be cached in the vCPU state, reducing the need for repeated exits.
  
- **Selective Interception**: Only intercept registers that require virtualization; allow others to execute natively using hardware support for "wildcard" trapping.

- **Paravirtualization Hints**: Future enhancements could introduce paravirtualized interfaces where guests voluntarily avoid problematic register accesses in exchange for higher performance.

Currently, every system register access triggers a full VM exit, so minimizing unnecessary interceptions is critical for maintaining acceptable performance.

**Section sources**
- [vm.rs](file://src/vm.rs#L424-L450)

## Debugging Strategies
Register-related issues often manifest as guest crashes, hangs, or incorrect behavior during boot or runtime. Effective debugging strategies include:

- **Exit Tracing**: Enable detailed tracing of `AxVCpuExitReason` events to log every system register access, including address, operation type, and involved registers.
  
- **Handler Coverage Validation**: Ensure all required system registers have appropriate handlers installed, especially during early boot phases where CPU initialization sequences occur.

- **Consistency Checking**: Validate that emulated register values adhere to architectural constraints (e.g., `CPUID` feature flags match expected capabilities).

- **Cross-Architecture Comparison**: Compare behavior across different ISAs (x86_64, AArch64, RISC-V) to identify missing or incorrect trap conditions.

Using structured logging and assertion checks within handler implementations helps isolate misconfigurations or unsupported operations quickly.

**Section sources**
- [vm.rs](file://src/vm.rs#L452-L487)