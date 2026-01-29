use alloc::vec::Vec;

use bitmap_allocator::{BitAlloc, BitAlloc4K};
use derive_more::From;
use spin::Mutex;

use super::percpu::PerCpuSet;
use crate::{
    arch::HCpu,
    hal::{ArchOp, HCpuOp},
};

pub(super) static PRE_CPU: PerCpuSet<HCpu> = PerCpuSet::new();
pub(super) static HCPU_ALLOC: Mutex<BitAlloc4K> = Mutex::new(BitAlloc4K::DEFAULT);
static CPU_LIST: spin::Once<Vec<CpuHardId>> = spin::Once::new();

pub fn count() -> usize {
    list().len()
}

pub fn list() -> Vec<CpuHardId> {
    CPU_LIST.call_once(|| crate::arch::Hal::cpu_list()).clone()
}

/// Exclusive access to a hardware CPU
#[derive(Debug)]
pub struct HCpuExclusive(CpuId);

impl HCpuExclusive {
    pub fn id(&self) -> CpuId {
        self.0
    }

    pub fn try_new(id: Option<CpuId>) -> Option<Self> {
        let mut a = HCPU_ALLOC.lock();
        match id {
            Some(id) => {
                // Try to allocate the specific ID
                let raw = a.alloc_contiguous(Some(id.raw()), 1, 0)?;
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
        for (_id, cpu) in PRE_CPU.iter() {
            if cpu.id == self.0 {
                return f(cpu);
            }
        }
        panic!("CPU data not found for CPU ID {}", self.0);
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

#[derive(
    derive_more::Debug,
    derive_more::Display,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    From,
)]
#[debug("CPU Hard({_0:#x})")]
#[display("CPU Hard({_0:#x})")]
#[repr(transparent)]
pub struct CpuHardId(usize);

impl CpuHardId {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}

#[derive(
    derive_more::Debug,
    derive_more::Display,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    From,
)]
#[debug("CPU({_0:#x})")]
#[display("CPU({_0:#x})")]
#[repr(transparent)]
pub struct CpuId(usize);

impl CpuId {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub fn raw(&self) -> usize {
        self.0
    }
}
