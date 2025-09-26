# Getting Started

<cite>
**Referenced Files in This Document**
- [lib.rs](file://src/lib.rs)
- [vm.rs](file://src/vm.rs)
- [config.rs](file://src/config.rs)
- [hal.rs](file://src/hal.rs)
- [Cargo.toml](file://Cargo.toml)
- [README.md](file://README.md)
</cite>

## Table of Contents
1. [Prerequisites](#prerequisites)
2. [VM Configuration](#vm-configuration)
3. [Creating and Initializing a VM](#creating-and-initializing-a-vm)
4. [Booting and Running a vCPU](#booting-and-running-a-vcpu)
5. [Minimal Working Example](#minimal-working-example)
6. [Common Pitfalls and Troubleshooting](#common-pitfalls-and-troubleshooting)

## Prerequisites

Before setting up a basic VM using axvm, ensure the following prerequisites are met:

- **Rust Toolchain**: Install the latest stable Rust toolchain via rustup. The crate uses edition 2024, so ensure compatibility.
- **ArceOS Environment**: Set up the ArceOS hypervisor environment, including required dependencies such as `axvcpu`, `axaddrspace`, and `axdevice`. These are specified in the Cargo.toml file.
- **Hardware Virtualization Support**: Ensure the host system supports hardware virtualization (Intel VT-x, AMD-V, or equivalent for ARM/RISC-V). The function `has_hardware_support()` in `lib.rs` checks this capability.
- **TOML-Based Configuration Understanding**: Familiarize yourself with TOML format, as VM configurations are derived from `AxVMCrateConfig` defined in TOML files and converted to `AxVMConfig`.

The axvm crate depends on several ArceOS components for CPU, memory, and device management, as outlined in the project’s README and Cargo.toml.

**Section sources**
- [lib.rs](file://src/lib.rs#L1-L32)
- [Cargo.toml](file://Cargo.toml#L1-L39)
- [README.md](file://README.md#L1-L7)

## VM Configuration

The VM configuration is managed through the `AxVMConfig` struct defined in `config.rs`. This structure can be created either programmatically or from a TOML-based configuration file via `AxVMCrateConfig`.

Key configuration fields include:
- `id`: Unique identifier for the VM.
- `cpu_num`: Number of vCPUs to allocate.
- `memory_regions`: List of guest physical address (GPA) ranges and their mapping types (`MapIdentical` or `MapAlloc`).
- `bsp_entry` and `ap_entry`: Entry points in GPA for bootstrap and application processors.
- `interrupt_mode`: Specifies whether interrupts are emulated or passed through.
- `pass_through_devices`: Device regions mapped directly into the guest.

The `From<AxVMCrateConfig>` implementation converts high-level TOML configurations into `AxVMConfig`, setting default values where necessary.

**Section sources**
- [config.rs](file://src/config.rs#L33-L103)

## Creating and Initializing a VM

To instantiate a VM, call `AxVM::new(config)` with a valid `AxVMConfig`. This method performs the following steps:

1. **vCPU Creation**: For each vCPU, an `AxVCpuRef` is created based on architecture-specific parameters (e.g., MPIDR_EL1 for AArch64, hart ID for RISC-V).
2. **Address Space Setup**: An `AddrSpace` is initialized with the base GPA and size. Memory regions are mapped according to their flags and mapping type.
3. **Device Initialization**: Emulated and pass-through devices are registered. On AArch64, SPIs are assigned if interrupt passthrough is enabled.
4. **vCPU Setup**: Each vCPU is configured with its entry point and two-stage page table root obtained via `ept_root()`.

The resulting `AxVMRef` is wrapped in `Arc` for shared ownership and thread-safe access.

**Section sources**
- [vm.rs](file://src/vm.rs#L74-L282)

## Booting and Running a vCPU

After creating the VM, initialize it by calling `AxVM::boot()`. This method:
- Checks for hardware virtualization support.
- Ensures the VM is not already running.
- Sets the `running` flag to true.

Once booted, execute a vCPU using `AxVM::run_vcpu(vcpu_id)`. This:
- Binds the vCPU to the current physical CPU.
- Enters a loop handling exit reasons such as MMIO reads/writes, I/O operations, and page faults.
- Delegates device interactions to `AxVmDevices`.
- Unbinds the vCPU upon exit.

The method returns an `AxVCpuExitReason`, indicating why execution was suspended (e.g., halt, external interrupt).

**Section sources**
- [vm.rs](file://src/vm.rs#L363-L424)

## Minimal Working Example

Below is a minimal example demonstrating VM setup and execution:

```rust
// Example assumes HAL implementation and valid config
let config = AxVMConfig {
    id: 1,
    cpu_num: 1,
    bsp_entry: GuestPhysAddr::from(0x80000),
    memory_regions: vec![VmMemConfig {
        gpa: 0x80000,
        size: 0x10000,
        flags: MappingFlags::READ | MappingFlags::WRITE,
        map_type: VmMemMappingType::MapAlloc,
    }],
    ..Default::default()
};

let vm = AxVM::<MyHal, MyVCpuHal>::new(config)?;
vm.boot()?;
let exit_reason = vm.run_vcpu(0)?;
info!("vCPU exited due to {:?}", exit_reason);
```

Expected output includes log messages indicating VM creation, booting, and vCPU exit reason. Success indicators are `Ok` results from `new`, `boot`, and `run_vcpu` calls.

**Section sources**
- [vm.rs](file://src/vm.rs#L74-L424)
- [config.rs](file://src/config.rs#L33-L103)

## Common Pitfalls and Troubleshooting

### Missing HAL Implementations
Ensure all methods in the `AxVMHal` trait are implemented, especially `alloc_memory_region_at`, `virt_to_phys`, and `inject_irq_to_vcpu`. Failure to implement these will result in compilation or runtime errors.

### Incorrect Memory Mappings
- Avoid using `MappingFlags::DEVICE` in regular memory regions; use `pass_through_devices` instead.
- Ensure memory region GPAs do not overlap unless intentionally merged.
- Use `MapIdentical` only when host and guest addresses align; otherwise, prefer `MapAlloc`.

### Hardware Support Issues
If `has_hardware_support()` returns false, verify:
- Virtualization features are enabled in BIOS/UEFI.
- Required CPU extensions (e.g., VMX, SVM, HVC) are present.
- The target architecture (x86_64, aarch64, riscv64) matches the host.

### vCPU Execution Failures
- Ensure `bind()` succeeds before `run()`.
- Check that the entry point (BSP/AP) is correctly set and accessible.
- Handle MMIO exits properly in emulated devices to prevent infinite loops.

For debugging, enable logging and inspect messages related to VM creation, memory setup, and vCPU exits.

**Section sources**
- [hal.rs](file://src/hal.rs#L1-L43)
- [vm.rs](file://src/vm.rs#L254-L282)
- [vm.rs](file://src/vm.rs#L363-L404)