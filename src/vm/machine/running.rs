use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    collections::btree_map::BTreeMap,
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    sync::Arc,
};

use alloc::vec::Vec;

use crate::{
    TASK_STACK_SIZE, VmAddrSpace, arch::cpu::VCpu, data::VmDataWeak, vcpu::VCpuOp,
    vhal::cpu::CpuHardId,
};

pub struct VmMachineRunningCommon {
    pub cpus: BTreeMap<CpuHardId, VCpu>,
    pub vmspace: VmAddrSpace,
    pub vm: VmDataWeak,
    running_cpu_count: Arc<AtomicUsize>,
}

impl VmMachineRunningCommon {
    pub fn new(vmspace: VmAddrSpace, vcpu: Vec<VCpu>, vm: VmDataWeak) -> Self {
        let mut cpus = BTreeMap::new();
        for cpu in vcpu.into_iter() {
            cpus.insert(cpu.hard_id(), cpu);
        }

        VmMachineRunningCommon {
            vmspace,
            cpus,
            vm,
            running_cpu_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn take_cpu(&mut self) -> anyhow::Result<VCpu> {
        let next = self
            .cpus
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow!("No CPUs available"))?;
        let cpu = self.cpus.remove(&next).unwrap();
        Ok(cpu)
    }

    pub fn run_cpu<C: VCpuOp>(&mut self, mut cpu: C) -> anyhow::Result<()> {
        let waiter = self.new_waiter();
        let thread_ok = Arc::new(AtomicBool::new(false));
        let thread_ok_clone = thread_ok.clone();
        let bind_id = cpu.bind_id();
        std::thread::Builder::new()
            .name(format!("init-cpu-{}", bind_id))
            .stack_size(TASK_STACK_SIZE)
            .spawn(move || {
                // Initialize cpu affinity here.
                assert!(
                    set_current_affinity(AxCpuMask::one_shot(bind_id.raw())),
                    "Initialize CPU affinity failed!"
                );
                thread_ok_clone.store(true, Ordering::SeqCst);

                info!(
                    "vCPU {} on {} ready, waiting for running...",
                    cpu.bind_id(),
                    bind_id
                );
                waiter.vm.wait_for_running();
                info!("VCpu {} on {} run", cpu.hard_id(), bind_id);
                // debug!("\n{:#x?}", cpu);
                let res = cpu.run();
                if let Err(e) = res {
                    info!("vCPU {} exited with error: {e}", bind_id);
                    if let Some(vm) = waiter.vm.upgrade() {
                        vm.set_err(e);
                    }
                }
                waiter.running_cpu_count.fetch_sub(1, Ordering::SeqCst);
                if waiter.running_cpu_count.load(Ordering::SeqCst) == 0 {
                    info!("All vCPUs have exited, VM set stopped.");
                    waiter.vm.set_stopped();
                }
            })
            .map_err(|e| anyhow!("{e:?}"))?;
        debug!("Waiting for CPU {} thread", bind_id);
        while !thread_ok.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        Ok(())
    }

    fn new_waiter(&self) -> Waiter {
        let running_cpu_count = self.running_cpu_count.clone();
        running_cpu_count.fetch_add(1, Ordering::SeqCst);
        Waiter {
            running_cpu_count,
            vm: self.vm.clone(),
        }
    }
}

struct Waiter {
    running_cpu_count: Arc<AtomicUsize>,
    vm: VmDataWeak,
}
