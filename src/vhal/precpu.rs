use alloc::collections::BTreeMap;
use core::{cell::UnsafeCell, ops::Deref};

use crate::{
    arch::Hal,
    vhal::{ArchHal, cpu::CpuHardId},
};

pub(crate) struct PreCpuSet<T>(UnsafeCell<BTreeMap<CpuHardId, Option<T>>>);

unsafe impl<T> Sync for PreCpuSet<T> {}
unsafe impl<T> Send for PreCpuSet<T> {}

impl<T> PreCpuSet<T> {
    pub const fn new() -> Self {
        PreCpuSet(UnsafeCell::new(BTreeMap::new()))
    }

    pub unsafe fn set(&self, cpu_id: CpuHardId, val: T) {
        let pre_cpu_map = unsafe { &mut *self.0.get() };
        pre_cpu_map.insert(cpu_id, Some(val));
    }

    pub fn init(&self) {
        let cpu_list = Hal::cpu_list();
        debug!("Initializing PreCpuSet for CPUs: {:?}", cpu_list);
        for cpu_id in cpu_list {
            let v = unsafe { &mut *self.0.get() };
            v.insert(cpu_id, None);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (CpuHardId, &T)> {
        let set = unsafe { &*self.0.get() };
        set.iter()
            .map(|(k, v)| (*k, v.as_ref().expect("CPU data not initialized!")))
    }
}

impl<T> Deref for PreCpuSet<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let set = unsafe { &*self.0.get() };
        let cpu_id = Hal::cpu_hard_id();
        let cpu_data = set
            .get(&cpu_id)
            .and_then(|data| data.as_ref())
            .expect("CPU data not initialized!");
        cpu_data
    }
}
