<h1 align="center">axvm</h1>

<p align="center">Virtual Machine Resource Management for ArceOS Hypervisors</p>

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/axvm.svg)](https://crates.io/crates/axvm)
[![Docs.rs](https://docs.rs/axvm/badge.svg)](https://docs.rs/axvm)
[![Rust](https://img.shields.io/badge/edition-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/arceos-hypervisor/axvm/blob/main/LICENSE)

</div>

English | [中文](README_CN.md)

# Introduction

`axvm` is a `#![no_std]` virtual machine resource management crate for ArceOS hypervisors. It coordinates guest address space setup, virtual CPU initialization, emulated device management, and VM lifecycle control on top of `axaddrspace`, `axvcpu`, `axdevice`, and `axvmconfig`.

This library exports five core public types:

- **`AxVM`** - Main virtual machine object
- **`AxVMRef`** - Shared reference type for an `AxVM`
- **`AxVCpuRef`** - Shared reference type for a VM vCPU
- **`VMMemoryRegion`** - Describes a mapped guest memory region
- **`VMStatus`** - Represents VM lifecycle states such as loading, running, and stopped

It also provides:

- **`config`** - VM configuration types such as `AxVMConfig` and `AxVMCrateConfig`
- **`AxVMPerCpu`** - Architecture-independent per-CPU virtualization helper type
- **`has_hardware_support()`** - Checks whether the current hardware supports virtualization

## Quick Start

### Requirements

- Rust nightly toolchain
- Rust components: rust-src, clippy, rustfmt

```bash
# Install rustup (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install nightly toolchain and components
rustup install nightly
rustup component add rust-src clippy rustfmt --toolchain nightly
```

### Run Check and Test

```bash
# 1. Enter the repository
cd axvm

# 2. Code check
./scripts/check.sh

# 3. Run tests
./scripts/test.sh
```

## Integration

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
axvm = "0.3.0"
```

### Example

```rust
use axvm::{
    config::{AxVMConfig, AxVMCrateConfig},
    AxVM, VMStatus,
};

fn main() {
    let crate_config = AxVMCrateConfig::default();
    let config = AxVMConfig::from(crate_config);

    let vm = AxVM::new(config).unwrap();
    assert_eq!(vm.vm_status(), VMStatus::Loading);

    // Initialize guest resources such as address space, vCPUs, and devices.
    vm.init().unwrap();

    // Boot the VM after initialization.
    vm.boot().unwrap();
    assert_eq!(vm.vm_status(), VMStatus::Running);

    // Query resources managed by the VM object.
    let _vcpu_num = vm.vcpu_num();
    let _ept_root = vm.ept_root();
    let _devices = vm.get_devices();
}
```

### Documentation

Generate and view API documentation:

```bash
cargo doc --no-deps --open
```

Online documentation: [docs.rs/axvm](https://docs.rs/axvm)

# Contributing

1. Fork the repository and create a branch
2. Run local check: `./scripts/check.sh`
3. Run local tests: `./scripts/test.sh`
4. Submit PR and pass CI checks

# License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
