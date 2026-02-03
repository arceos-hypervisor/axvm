use std::{sync::Arc, thread::JoinHandle};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};

use spin::Mutex;

use crate::{
    CpuHardId, GuestPhysAddr, VmWeak,
    hal::HalOp,
    vcpu::{CpuBootInfo, VCpu},
};

pub struct VCpuList<H: HalOp>(Arc<Mutex<Inner<H>>>);

struct Inner<H: HalOp> {
    vcpus: BTreeMap<CpuHardId, VCpu<H>>,
    threads: Vec<JoinHandle<VCpu<H>>>,
    vm: VmWeak,
}

impl<H: HalOp> VCpuList<H> {
    pub fn new(vcpus: Vec<VCpu<H>>, main_thread: JoinHandle<VCpu<H>>, vm: VmWeak) -> Self {
        let mut vcpu_map = BTreeMap::new();
        for vcpu in vcpus {
            vcpu_map.insert(vcpu.hard_id.clone(), vcpu);
        }
        let threads = vec![main_thread];

        Self(Arc::new(Mutex::new(Inner {
            vcpus: vcpu_map,
            threads,
            vm,
        })))
    }

    pub fn cpu_up(
        &self,
        target_cpu: CpuHardId,
        entry_point: GuestPhysAddr,
        arg: usize,
    ) -> anyhow::Result<()> {
        let mut inner = self.0.lock();
        let mut cpu = inner
            .vcpus
            .remove(&target_cpu)
            .ok_or_else(|| anyhow!("Target CPU not found"))?;
        let info = cpu.get_boot_info();

        cpu.set_boot_info(&CpuBootInfo {
            kernel_entry: entry_point,
            secondary_boot_arg: Some(arg),
            ..info
        })?;
        let vm = inner.vm.clone();
        inner.threads.push(cpu.run_in_thread(vm, false)?);

        Ok(())
    }
}
