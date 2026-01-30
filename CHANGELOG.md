# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-01-30

### Added

- Initial release of `axvm` crate
- `AxVM` structure for virtual machine management:
  - VM lifecycle management (Loading, Loaded, Running, Suspended, Stopping, Stopped)
  - vCPU creation and management
  - Guest memory mapping and address translation
  - Device emulation support (MMIO, Port I/O, System Registers)
  - Passthrough device support
  - IVC (Inter-VM Communication) channel support
- `AxVMHal` trait for hardware abstraction layer
- `VMStatus` enumeration for VM lifecycle states
- `VMMemoryRegion` structure for memory region management
- Configuration module with:
  - `AxVMConfig` for VM configuration
  - `AxVCpuConfig` for vCPU configuration
  - `VMImageConfig` for VM image configuration
  - `PhysCpuList` for physical CPU management
- Multi-architecture support:
  - x86_64 with VMX (Intel VT-x)
  - AArch64 with ARM virtualization extensions
  - RISC-V64 with H-extension
- Resource cleanup and Drop implementation for proper VM teardown

[Unreleased]: https://github.com/arceos-hypervisor/axvm/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/arceos-hypervisor/axvm/releases/tag/v0.1.0
