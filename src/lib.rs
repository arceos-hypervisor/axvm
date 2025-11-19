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

const TASK_STACK_SIZE: usize = 0x40000; // 16KB

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch/x86_64/mod.rs")]
pub mod arch;

mod fdt;
mod vcpu;
mod vm;

pub mod config;
pub mod vhal;

/// Enable hardware virtualization support.
pub fn enable_viretualization() -> anyhow::Result<()> {
    vhal::init()?;
    Ok(())
}
