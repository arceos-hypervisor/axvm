#![no_std]
#![feature(new_range_api)]
// #![feature(concat_idents)]
// #![feature(naked_functions)]
// #![feature(const_trait_impl)]

//! This crate provides a minimal VM monitor (VMM) for running guest VMs.
//!
//! This crate contains:
//! - [`AxVM`]: The main structure representing a VM.

extern crate alloc;
#[macro_use]
extern crate log;

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch/x86_64/mod.rs")]
pub mod arch;

mod fdt;
mod hal;
mod vcpu;
mod vm;
mod vm2;

pub mod config;
pub mod vhal;

use anyhow::bail;
pub use hal::AxVMHal;
pub use vm::AxVCpuRef;
pub use vm::AxVM;
pub use vm::AxVMRef;
pub use vm::VMMemoryRegion;

/// The architecture-independent per-CPU type.
pub type AxVMPerCpu<U> = axvcpu::AxPerCpu<vcpu::AxVMArchPerCpuImpl<U>>;

/// Whether the hardware has virtualization support.
pub fn has_hardware_support() -> bool {
    vcpu::has_hardware_support()
}

