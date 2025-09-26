# Introduction

<cite>
**Referenced Files in This Document**   
- [README.md](file://README.md)
- [src/lib.rs](file://src/lib.rs)
- [src/config.rs](file://src/config.rs)
- [src/vm.rs](file://src/vm.rs)
- [src/hal.rs](file://src/hal.rs)
</cite>

## Table of Contents
1. [Introduction](#introduction)
2. [Core Components](#core-components)
3. [VM Lifecycle Management](#vm-lifecycle-management)
4. [Resource Management](#resource-management)
5. [Example: VM Instantiation Workflow](#example-vm-instantiation-workflow)

## Core Components

The axvm crate serves as a virtual machine resource management library within the ArceOS hypervisor ecosystem, providing essential infrastructure for creating and managing guest VMs across multiple architectures including x86_64, riscv64, and aarch64. The crate implements RAII-based lifecycle management for virtualized resources with emphasis on vCPU, memory, and device management.

At the core of the system is the `AxVM` structure, which represents a complete virtual machine instance and manages its resources throughout its lifecycle. This main VM structure coordinates between various components to provide a unified interface for VM operations. The `AxVMConfig` struct defines the configuration parameters for VM creation, containing specifications for CPU topology, memory layout, device configurations, and boot parameters. These configurations are typically derived from TOML files through the `AxVMCrateConfig` intermediate format before being converted into the runtime `AxVMConfig`.

The `AxVMHal` trait defines the hardware abstraction layer that must be implemented by the underlying system (hypervisor or kernel), specifying critical interfaces for physical address management, interrupt injection, and timekeeping. This trait enables the axvm crate to operate in a platform-agnostic manner while still accessing necessary low-level functionality. The implementation requires methods for allocating and deallocating memory regions, converting virtual to physical addresses, retrieving timing information, and managing inter-processor interrupts.

**Section sources**
- [src/lib.rs](file://src/lib.rs#L0-L32)
- [src/config.rs](file://src/config.rs#L48-L195)
- [src/hal.rs](file://src/hal.rs#L4-L44)

## VM Lifecycle Management

The axvm crate implements a well-defined VM lifecycle with distinct states and transition methods. VM creation begins with the `new` method on the `AxVM` struct, which takes an `AxVMConfig` parameter and performs initial setup of vCPUs, memory mappings, and device configurations. During this phase, the system validates hardware virtualization support through the `has_hardware_support()` function before proceeding with resource allocation.

Once created, a VM exists in a non-running state until explicitly booted via the `boot()` method. This two-stage initialization process allows for comprehensive configuration and validation before execution begins. The boot process sets up each vCPU with appropriate entry points (BSP for bootstrap processor, AP for application processors) and initializes the two-stage address translation page table structure. The VM maintains atomic state flags to track whether it is currently running or in the process of shutting down, preventing invalid state transitions.

The shutdown procedure is initiated through the `shutdown()` method, which marks the VM as terminating. Notably, the current implementation does not support re-initialization of a VM after shutdown, reflecting a design decision to simplify resource cleanup and prevent potential state corruption. Future enhancements may include suspend/resume capabilities, but these are currently marked as TODO items in the codebase.

**Section sources**
- [src/vm.rs](file://src/vm.rs#L75-L404)
- [src/lib.rs](file://src/lib.rs#L30-L32)

## Resource Management

The axvm crate provides comprehensive resource management capabilities for virtual machine components. For vCPU management, the system creates an array of `AxVCpuRef` instances during VM initialization, with each representing a virtual CPU that can be individually controlled and executed. The vCPU setup process configures architecture-specific parameters such as MPIDR_EL1 values on aarch64 or hart IDs on riscv64, ensuring proper identification within the guest environment.

Memory management leverages the `axaddrspace` crate to implement two-stage address translation, mapping guest physical addresses (GPA) to host physical addresses (HPA). The system supports different memory mapping types including identical mapping (`MapIdentical`) where GPA equals HPA, and allocated mapping (`MapAlloc`) where physical memory is dynamically assigned. Special handling is provided for pass-through devices, where memory regions are aligned to 4K boundaries and overlapping regions are merged to optimize the address space layout.

Device management combines emulated devices and pass-through devices, with special consideration for interrupt handling modes. In passthrough mode, the system assigns shared peripheral interrupts (SPIs) to specific CPUs through the VGicD controller on aarch64 platforms. For non-passthrough mode, virtual timer devices are set up to provide timekeeping services to the guest. Inter-VM communication is supported through IVC (Inter-VM Communication) channels, which can be allocated and released using dedicated methods that handle the underlying memory allocation and deallocation.

**Section sources**
- [src/vm.rs](file://src/vm.rs#L100-L330)
- [src/vm.rs](file://src/vm.rs#L489-L626)

## Example: VM Instantiation Workflow

The typical workflow for instantiating a VM using the axvm crate follows a structured sequence of operations. First, configuration data is loaded, typically from a TOML file that defines the VM's properties including ID, name, CPU count, memory regions, and device configurations. This configuration is parsed into an `AxVMCrateConfig` structure and then converted to the runtime `AxVMConfig` used by the VM creation process.

The VM is then created by calling `AxVM::new(config)`, which performs several critical initialization steps: validating hardware virtualization support, setting up the address space with specified memory regions, creating vCPU instances with appropriate affinities, and configuring devices according to the specification. During this phase, the system establishes the two-stage address translation page tables and prepares the emulated device framework.

After successful creation, the VM remains in a stopped state until explicitly booted. The boot process involves calling the `boot()` method, which verifies that the hardware supports virtualization and that the VM is not already running. Once booted, individual vCPUs can be executed using the `run_vcpu(vcpu_id)` method, which handles vCPU binding, execution, and exit reason processing. The system uses RAII principles to ensure proper resource cleanup when the VM reference count reaches zero, automatically releasing allocated memory and other resources.

**Section sources**
- [README.md](file://README.md#L0-L7)
- [src/lib.rs](file://src/lib.rs#L0-L32)
- [src/vm.rs](file://src/vm.rs#L75-L404)