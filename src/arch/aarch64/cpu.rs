use axhal::percpu::this_cpu_id;

use crate::{
    fdt::{self, fdt},
    vhal::{ArchHal, PreCpuSet},
};

static PRE_CPU: PreCpuSet<PreCpu> = PreCpuSet::new();

struct PreCpu;

pub fn init() -> anyhow::Result<()> {
    PRE_CPU.init();
    todo!();
    Ok(())
}
