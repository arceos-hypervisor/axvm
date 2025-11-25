use core::fmt::Display;

use bitmap_allocator::{BitAlloc, BitAlloc4K};
use spin::Mutex;

use crate::{
    arch::HCpu,
    vhal::{ArchCpuData, precpu::PreCpuSet},
};

pub(super) static PRE_CPU: PreCpuSet<HCpu> = PreCpuSet::new();
pub(super) static HCPU_ALLOC: Mutex<BitAlloc4K> = Mutex::new(BitAlloc4K::DEFAULT);

pub struct HCpuExclusive(CpuId);

impl HCpuExclusive {
    pub fn try_new(id: Option<CpuId>) -> Option<Self> {
        let mut a = HCPU_ALLOC.lock();
        match id {
            Some(id) => {
                // Try to allocate the specific ID
                let raw = a.alloc_contiguous(Some(id.raw()), 1, 1)?;
                Some(HCpuExclusive(CpuId::new(raw)))
            }
            None => {
                // Auto-allocate any available ID
                let raw_id = a.alloc()?;
                Some(HCpuExclusive(CpuId::new(raw_id)))
            }
        }
    }

    pub fn with_cpu<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HCpu) -> R,
    {
        unsafe {
            for (id, cpu) in PRE_CPU.iter() {
                if cpu.id == self.0 {
                    return f(cpu);
                }
            }
        }
        panic!("CPU data not found for CPU ID {}", self.0);
    }

    pub fn cpu_id(&self) -> CpuId {
        self.0
    }

    pub fn hard_id(&self) -> CpuHardId {
        self.with_cpu(|cpu| cpu.hard_id())
    }
}

impl Drop for HCpuExclusive {
    fn drop(&mut self) {
        let mut allocator = HCPU_ALLOC.lock();
        allocator.dealloc(self.0.raw());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CpuHardId(usize);

impl CpuHardId {
    pub fn new(id: usize) -> Self {
        CpuHardId(id)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}

impl Display for CpuHardId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CPU Hard({:#x})", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CpuId(usize);

impl CpuId {
    pub fn new(id: usize) -> Self {
        CpuId(id)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}

impl Display for CpuId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CPU({})", self.0)
    }
}
