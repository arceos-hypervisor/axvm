use aarch64_cpu_ext::cache::{CacheOp, dcache_range};

pub mod cpu;
mod hal;
mod vm;

pub use cpu::HCpu;
pub use hal::Hal;
pub use vm::*;

type AddrSpace = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;
