# axvm

[![Crates.io](https://img.shields.io/crates/v/axvm)](https://crates.io/crates/axvm)
[![Docs.rs](https://docs.rs/axvm/badge.svg)](https://docs.rs/axvm)
[![CI](https://github.com/arceos-hypervisor/axvm/actions/workflows/deploy.yml/badge.svg)](https://github.com/arceos-hypervisor/axvm/actions/workflows/deploy.yml)

Virtual Machine resource management crate for [ArceOS](https://github.com/arceos-org/arceos) Hypervisor ([AxVisor](https://github.com/arceos-hypervisor/axvisor)).

## Overview

`axvm` provides the core abstractions for managing virtual machines (VMs) in the AxVisor hypervisor. It handles VM lifecycle, vCPU management, memory mapping, and device emulation.

## Features

- **`no_std` compatible**: Designed for bare-metal and hypervisor environments
- **Multi-architecture support**: x86_64 (VMX), AArch64, RISC-V64
- **VM lifecycle management**: Loading, running, suspending, stopping states
- **Memory management**: Guest physical address space with two-stage translation
- **Device support**: Both emulated and passthrough devices
- **Inter-VM communication**: IVC channel support for VM-to-VM communication

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                        AxVM                             │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │    vCPUs    │  │   Memory    │  │     Devices     │  │
│  │  (axvcpu)   │  │(axaddrspace)│  │   (axdevice)    │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
├─────────────────────────────────────────────────────────┤
│                    Architecture Specific                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │  x86_vcpu   │  │  arm_vcpu   │  │   riscv_vcpu    │  │
│  │    (VMX)    │  │  (ARM VHE)  │  │  (H-extension)  │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## VM Resources

Each VM manages the following resources:

| Resource | Crate | Description |
|----------|-------|-------------|
| vCPUs | [axvcpu](https://github.com/arceos-hypervisor/axvcpu) | Virtual CPU instances |
| Memory | [axaddrspace](https://github.com/arceos-hypervisor/axaddrspace) | Guest physical address space |
| Devices | [axdevice](https://github.com/arceos-hypervisor/axdevice) | Emulated and passthrough devices |

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
axvm = "0.1"
```

### Creating and Running a VM

```rust,ignore
use axvm::{AxVM, AxVMHal, config::AxVMConfig};

// Create a VM with the given configuration
let config = AxVMConfig::from(vm_crate_config);
let vm = AxVM::<MyHal, MyVCpuHal>::new(config)?;

// Initialize the VM (setup vCPUs, devices, memory)
vm.init()?;

// Boot the VM
vm.boot()?;

// Run a vCPU in a loop
loop {
    let exit_reason = vm.run_vcpu(0)?;
    match exit_reason {
        AxVCpuExitReason::Halt => break,
        // Handle other exit reasons...
        _ => {}
    }
}

// Shutdown the VM
vm.shutdown()?;
```

### VM Status

The VM can be in one of the following states:

| Status | Description |
|--------|-------------|
| `Loading` | VM is being created/loaded |
| `Loaded` | VM is loaded but not yet started |
| `Running` | VM is currently running |
| `Suspended` | VM is paused but can be resumed |
| `Stopping` | VM is in the process of shutting down |
| `Stopped` | VM is stopped |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `vmx` | Enable VMX (Intel VT-x) support (default) |
| `4-level-ept` | Enable 4-level EPT page table support |

## Supported Platforms

| Architecture | Virtualization Extension | Status |
|--------------|--------------------------|--------|
| x86_64 | Intel VT-x (VMX) | ✓ |
| AArch64 | ARM VHE | ✓ |
| RISC-V64 | H-extension | ✓ |

## Documentation

For detailed API documentation, visit [docs.rs/axvm](https://docs.rs/axvm).

## Related Crates

- [axvcpu](https://github.com/arceos-hypervisor/axvcpu) - Virtual CPU abstraction
- [axaddrspace](https://github.com/arceos-hypervisor/axaddrspace) - Guest address space management
- [axdevice](https://github.com/arceos-hypervisor/axdevice) - Device emulation framework
- [axvmconfig](https://github.com/arceos-hypervisor/axvmconfig) - VM configuration parsing

## License

This project is licensed under multiple licenses. You may choose to use this project under any of the following licenses:

- [GPL-3.0-or-later](LICENSE.GPLv3)
- [Apache-2.0](LICENSE.Apache2)
- [MulanPSL-2.0](LICENSE.MulanPSL2)
