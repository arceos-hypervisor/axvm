use std::{collections::btree_map::BTreeMap, thread::JoinHandle, vec::Vec};

use crate::{
    CpuHardId, VmAddrSpace, VmWeak,
    hal::HalOp,
    vcpu::{VCpu, VCpuList},
};

pub struct StateRunning<H: HalOp> {
    vmspace: VmAddrSpace,
    pub(crate) vm: VmWeak,
    pub(crate) vcpus: VCpuList<H>,
    plat: H::PlatData,
}

impl<H: HalOp> StateRunning<H> {
    pub fn new(
        main_cpu: JoinHandle<VCpu<H>>,
        cpus: Vec<VCpu<H>>,
        vmspace: VmAddrSpace,
        vm: VmWeak,
        plat: H::PlatData,
    ) -> anyhow::Result<Self> {
        let vcpus = VCpuList::new(cpus, main_cpu, vm.clone());

        Ok(Self {
            vmspace,
            vm,
            vcpus,
            plat,
        })
    }
}
