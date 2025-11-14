use core::cell::UnsafeCell;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};

use crate::arch::{self, Hal};

pub fn init() -> anyhow::Result<()> {
    Hal::init()
}

pub(crate) trait ArchHal {
    fn init() -> anyhow::Result<()>;
    fn cpu_list() -> Vec<CpuHardId>;
    fn current_enable_viretualization() -> anyhow::Result<()>;
}

pub fn current_enable_viretualization() -> anyhow::Result<()> {
    Hal::current_enable_viretualization()
}

pub(crate) struct PreCpuSet<T>(UnsafeCell<BTreeMap<usize, Option<T>>>);

unsafe impl<T> Sync for PreCpuSet<T> {}
unsafe impl<T> Send for PreCpuSet<T> {}

impl<T> PreCpuSet<T> {
    pub const fn new() -> Self {
        PreCpuSet(UnsafeCell::new(BTreeMap::new()))
    }

    unsafe fn set(&self, cpu_id: usize, val: T) {
        let pre_cpu_map = unsafe { &mut *self.0.get() };
        pre_cpu_map.insert(cpu_id, Some(val));
    }

    pub fn get(&self, cpu_id: usize) -> Option<&T> {
        let pre_cpu_map = unsafe { &*self.0.get() };
        let v = pre_cpu_map.get(&cpu_id)?;
        Some(v.as_ref().expect("init not called"))
    }

    pub fn init(&self) {
        let cpu_list = Hal::cpu_list();
        debug!("Initializing PreCpuSet for CPUs: {:?}", cpu_list);
        for cpu_id in cpu_list {
            unsafe {
                let v = unsafe { &mut *self.0.get() };
                v.insert(cpu_id.raw(), None);
            }
        }
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
