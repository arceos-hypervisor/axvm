#![no_std]
#![feature(new_range_api)]
#![warn(missing_docs)]

//! Virtual Machine resource management crate for ArceOS Hypervisor.
//!
//! This crate provides the core abstractions for managing virtual machines (VMs)
//! in the [AxVisor](https://github.com/arceos-hypervisor/axvisor) hypervisor.
//! It handles VM lifecycle, vCPU management, memory mapping, and device emulation.
//!
//! # Overview
//!
//! The main components provided by this crate are:
//!
//! - [`AxVM`]: The main structure representing a virtual machine, managing its
//!   lifecycle, memory, vCPUs, and devices.
//! - [`AxVMHal`]: Hardware abstraction layer trait that must be implemented by
//!   the underlying system.
//! - [`VMStatus`]: Enumeration representing the VM lifecycle states.
//! - [`config`]: Configuration structures for VM setup.
//!
//! # Architecture Support
//!
//! This crate supports multiple architectures through conditional compilation:
//!
//! - **x86_64**: Uses VMX (Intel VT-x) through `x86_vcpu` crate
//! - **AArch64**: Uses ARM virtualization extensions through `arm_vcpu` crate
//! - **RISC-V64**: Uses H-extension through `riscv_vcpu` crate
//!
//! # VM Resources
//!
//! Each VM manages the following resources:
//!
//! - **vCPUs**: Virtual CPU instances via [`axvcpu`](https://github.com/arceos-hypervisor/axvcpu)
//! - **Memory**: Guest physical address space via [`axaddrspace`](https://github.com/arceos-hypervisor/axaddrspace)
//! - **Devices**: Emulated and passthrough devices via [`axdevice`](https://github.com/arceos-hypervisor/axdevice)
//!
//! # Example
//!
//! ```rust,ignore
//! use axvm::{AxVM, AxVMHal, config::AxVMConfig};
//!
//! // Create a VM with the given configuration
//! let config = AxVMConfig::from(vm_crate_config);
//! let vm = AxVM::<MyHal, MyVCpuHal>::new(config)?;
//!
//! // Initialize and boot the VM
//! vm.init()?;
//! vm.boot()?;
//!
//! // Run a vCPU
//! let exit_reason = vm.run_vcpu(0)?;
//! ```
//!
//! # Features
//!
//! - `vmx`: Enable VMX (Intel VT-x) support (default)
//! - `4-level-ept`: Enable 4-level EPT page table support

extern crate alloc;
#[macro_use]
extern crate log;

mod hal;
mod vcpu;
mod vm;

#[cfg(test)]
mod tests;

pub mod config;

pub use hal::AxVMHal;
pub use vm::AxVCpuRef;
pub use vm::AxVM;
pub use vm::AxVMRef;
pub use vm::VMMemoryRegion;
pub use vm::VMStatus;

/// The architecture-independent per-CPU type.
pub type AxVMPerCpu<U> = axvcpu::AxPerCpu<vcpu::AxVMArchPerCpuImpl<U>>;

/// Whether the hardware has virtualization support.
pub fn has_hardware_support() -> bool {
    vcpu::has_hardware_support()
}
