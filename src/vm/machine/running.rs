use std::{thread::JoinHandle, vec::Vec};

use crate::{VmAddrSpace, VmWeak, hal::ArchOp, vcpu::VCpu};

pub struct StateRunning<H: ArchOp> {
    vmspace: VmAddrSpace,
    vm: VmWeak,
    threads: Vec<JoinHandle<VCpu<H>>>,
}

impl<H: ArchOp> StateRunning<H> {
    pub fn new(
        main_cpu: JoinHandle<VCpu<H>>,
        vmspace: VmAddrSpace,
        vm: VmWeak,
    ) -> anyhow::Result<Self> {
        let vcpus = vec![main_cpu];

        Ok(Self {
            vmspace,
            vm,
            threads: vcpus,
        })
    }
}
