// Copyright 2025 The Axvisor Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
