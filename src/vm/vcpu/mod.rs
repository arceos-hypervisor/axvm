use core::sync::atomic::{AtomicBool, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    sync::Arc,
    thread::JoinHandle,
};

use axvmconfig::VMInterruptMode;

use crate::{
    CpuId, GuestPhysAddr, HostPhysAddr, RunError, TASK_STACK_SIZE, Vm, VmWeak,
    arch::HCpu,
    hal::{
        ArchOp, HCpuOp,
        cpu::{CpuHardId, HCpuExclusive},
    },
};

#[derive(Default, Clone)]
pub(crate) struct CpuBootInfo {
    pub kernel_entry: GuestPhysAddr,
    pub dtb_addr: GuestPhysAddr,
    pub gpt_root: HostPhysAddr,
    pub pt_levels: usize,
    pub pa_bits: usize,
    pub irq_mode: VMInterruptMode,
    pub secondary_boot_arg: Option<usize>,
}

pub(crate) trait VCpuOp: core::fmt::Debug + Send + 'static {
    fn set_boot_info(&mut self, info: &CpuBootInfo) -> anyhow::Result<()>;
    fn get_boot_info(&self) -> CpuBootInfo;
    fn run(&mut self, vm: &Vm) -> Result<(), RunError>;
}

pub struct VCpu<H: ArchOp> {
    pub id: CpuId,
    pub hard_id: CpuHardId,
    pub vm: VmWeak,
    pub hcpu_exclusive: HCpuExclusive,
    inner: H::VCPU,
    is_primary: bool,
}

impl<H: ArchOp> VCpu<H> {
    pub fn new(bind_id: Option<CpuId>, vm: VmWeak) -> anyhow::Result<Self> {
        let hcpu_exclusive = HCpuExclusive::try_new(bind_id)
            .ok_or_else(|| anyhow!("Failed to allocate hardware CPU for bind id {bind_id:?}"))?;
        let hard_id = hcpu_exclusive.hard_id();
        let id = hcpu_exclusive.id();

        let inner = H::new_vcpu(hard_id.clone(), vm.clone())?;

        Ok(Self {
            id,
            hard_id,
            vm,
            hcpu_exclusive,
            inner,
            is_primary: false,
        })
    }

    pub fn bind_id(&self) -> CpuId {
        self.id
    }

    pub fn hcpu(&self) -> &impl HCpuOp {
        self.hcpu_exclusive.cpu()
    }

    pub fn set_boot_info(&mut self, info: &CpuBootInfo) -> anyhow::Result<()> {
        self.inner.set_boot_info(info)
    }

    pub fn get_boot_info(&self) -> CpuBootInfo {
        self.inner.get_boot_info()
    }

    pub fn run(&mut self) -> Result<(), RunError> {
        let vm = self.vm.upgrade().ok_or(RunError::Exit)?;
        self.inner.run(&vm)
    }

    pub fn run_in_thread(
        mut self,
        vm: VmWeak,
        is_primary: bool,
    ) -> anyhow::Result<JoinHandle<VCpu<H>>> {
        self.is_primary = is_primary;
        let thread_ok = Arc::new(AtomicBool::new(false));
        let thread_ok_clone = thread_ok.clone();
        let bind_id = self.bind_id();
        let hard_id = self.hard_id;
        let handle = std::thread::Builder::new()
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
                    self.hard_id, bind_id
                );
                vm.wait_for_running();
                info!("VCpu {} on {} run", self.hard_id, bind_id);
                // debug!("\n{:#x?}", cpu);
                let res = self.run();
                if let Err(e) = res {
                    info!("vCPU {} exited with error: {e}", bind_id);
                    vm.set_exit(Some(e));
                }
                let cpu_count = vm.stat.running_vcpu_count.fetch_sub(1, Ordering::SeqCst) - 1;
                debug!("vCPU {} exited, {} vCPUs remaining", bind_id, cpu_count);

                if cpu_count == 0 {
                    info!("All vCPUs have exited, VM set stopped.");
                    vm.set_exit(None);
                }

                self
            })
            .map_err(|e| anyhow!("{e:?}"))?;
        debug!("Waiting for CPU {} {} thread", bind_id, hard_id);
        while !thread_ok.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        Ok(handle)
    }
}
