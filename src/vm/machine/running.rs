use std::{collections::btree_map::BTreeMap, thread::JoinHandle, vec::Vec};

use crate::{
    CpuHardId, VmAddrSpace, VmWeak,
    hal::ArchOp,
    vcpu::{VCpu, VCpuList},
};

pub struct StateRunning<H: ArchOp> {
    vmspace: VmAddrSpace,
    pub(crate) vm: VmWeak,
    pub(crate) vcpus: VCpuList<H>,
}

impl<H: ArchOp> StateRunning<H> {
    pub fn new(
        main_cpu: JoinHandle<VCpu<H>>,
        cpus: Vec<VCpu<H>>,
        vmspace: VmAddrSpace,
        vm: VmWeak,
    ) -> anyhow::Result<Self> {
        let vcpus = VCpuList::new(cpus, main_cpu, vm.clone());

        Ok(Self { vmspace, vm, vcpus })
    }
}
