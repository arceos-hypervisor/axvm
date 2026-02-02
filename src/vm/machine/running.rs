use std::{collections::btree_map::BTreeMap, thread::JoinHandle, vec::Vec};

use crate::{CpuHardId, VmAddrSpace, VmWeak, hal::ArchOp, vcpu::VCpu};

pub struct StateRunning<H: ArchOp> {
    vmspace: VmAddrSpace,
    pub(crate) vm: VmWeak,
    pub(crate) vcpus: BTreeMap<CpuHardId, VCpu<H>>,
    threads: Vec<JoinHandle<VCpu<H>>>,
}

impl<H: ArchOp> StateRunning<H> {
    pub fn new(
        main_cpu: JoinHandle<VCpu<H>>,
        cpus: Vec<VCpu<H>>,
        vmspace: VmAddrSpace,
        vm: VmWeak,
    ) -> anyhow::Result<Self> {
        let threads = vec![main_cpu];
        let mut vcpus = BTreeMap::new();
        for cpu in cpus {
            vcpus.insert(cpu.hard_id, cpu);
        }

        Ok(Self {
            vmspace,
            vm,
            threads,
            vcpus,
        })
    }
}
