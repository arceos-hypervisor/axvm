use std::{collections::btree_map::BTreeMap, thread::JoinHandle, vec::Vec};

use crate::{
    CpuHardId, MmioRegions, VmAddrSpace, VmWeak,
    hal::HalOp,
    vcpu::{VCpu, VCpuList},
    vdev::VDeviceList,
};

pub struct StateRunning<H: HalOp> {
    vmspace: VmAddrSpace,
    pub(crate) vm: VmWeak,
    pub(crate) vcpus: VCpuList<H>,
    plat: H::PlatData,
    vdevs: VDeviceList,
    pub(crate) mmio_map: MmioRegions,
}

impl<H: HalOp> StateRunning<H> {
    pub fn new(
        main_cpu: JoinHandle<VCpu<H>>,
        cpus: Vec<VCpu<H>>,
        vmspace: VmAddrSpace,
        vm: VmWeak,
        plat: H::PlatData,
        vdevs: VDeviceList,
    ) -> anyhow::Result<Self> {
        let vcpus = VCpuList::new(cpus, main_cpu, vm.clone());
        let mmio_map = vmspace.mmio_map();
        Ok(Self {
            vmspace,
            vm,
            vcpus,
            plat,
            vdevs,
            mmio_map,
        })
    }
}
