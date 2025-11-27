#![no_std]
#![feature(new_range_api)]
// #![feature(concat_idents)]
// #![feature(naked_functions)]
// #![feature(const_trait_impl)]

//! This crate provides a minimal VM monitor (VMM) for running guest VMs.
//!
//! This crate contains:
//! - [`AxVM`]: The main structure representing a VM.

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;

extern crate axstd as std;

const TASK_STACK_SIZE: usize = 0x40000; // 256 KB

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch/x86_64/mod.rs")]
pub(crate) mod arch;

mod fdt;
mod vcpu;
mod vm;
mod region;

pub mod config;
pub mod vhal;

pub use axvm_types::addr::*;
pub use config::AxVMConfig;
pub use vhal::cpu::CpuId;
pub use vm::*;

/// Enable hardware virtualization support.
pub fn enable_viretualization() -> anyhow::Result<()> {
    vhal::init()?;
    Ok(())
}
