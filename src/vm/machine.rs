use alloc::{
    collections::VecDeque,
    string::{String, ToString},
    sync::Arc,
};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
use spin::Mutex;
use std::{
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    RunError, Status, VmId, VmStatusInitOps, VmStatusRunningOps,
    arch::{VmInit, VmStatusRunning, VmStatusStopping},
};

/// Default interval in milliseconds for polling the VM status from
/// the background machine thread.
const STATE_POLL_INTERVAL_MS: u64 = 20;

/// A lightweight container that stores the identifier and human readable name
/// for a VM instance. Shared between the public [`Vm`] object and the
/// background machine thread for logging and observability.
#[derive(Debug, Clone)]
pub struct VmCommon {
    pub id: VmId,
    pub name: String,
}

#[derive(Clone)]
struct CommandResponder {
    inner: Arc<CommandResponderInner>,
}

struct CommandResponderInner {
    ready: AtomicBool,
    worker_alive: Arc<AtomicBool>,
    result: Mutex<Option<anyhow::Result<()>>>,
}

impl CommandResponder {
    fn new(worker_alive: &Arc<AtomicBool>) -> Self {
        Self {
            inner: Arc::new(CommandResponderInner {
                ready: AtomicBool::new(false),
                worker_alive: worker_alive.clone(),
                result: Mutex::new(None),
            }),
        }
    }

    fn complete(&self, result: anyhow::Result<()>) {
        *self.inner.result.lock() = Some(result);
        self.inner.ready.store(true, Ordering::Release);
    }

    fn wait(self) -> anyhow::Result<()> {
        loop {
            if self.inner.ready.load(Ordering::Acquire) {
                return self.inner.result.lock().take().unwrap_or_else(|| Ok(()));
            }
            if !self.inner.worker_alive.load(Ordering::Acquire) {
                return Err(anyhow::anyhow!(
                    "vm worker stopped before completing command"
                ));
            }
            thread::yield_now();
        }
    }
}

enum MachineCommand {
    Start { responder: CommandResponder },
    Shutdown { responder: CommandResponder },
}

pub struct CommandMailbox {
    queue: Mutex<VecDeque<MachineCommand>>,
}

impl CommandMailbox {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, cmd: MachineCommand) {
        self.queue.lock().push_back(cmd);
    }

    pub fn pop(&self) -> Option<MachineCommand> {
        self.queue.lock().pop_front()
    }
}

#[derive(Clone)]
pub struct VmHandle {
    pub common: VmCommon,
    state: Arc<AtomicState>,
    commands: Arc<CommandMailbox>,
    worker_alive: Arc<AtomicBool>,
}

impl VmHandle {
    fn new(vm: &VmInit) -> Self {
        Self {
            common: VmCommon {
                id: vm.id(),
                name: vm.name().to_string(),
            },
            state: Arc::new(AtomicState::new(VMStatus::Loaded)),
            commands: Arc::new(CommandMailbox::new()),
            worker_alive: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn status(&self) -> VMStatus {
        self.state.load()
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let responder = CommandResponder::new(&self.worker_alive);
        self.send_command(MachineCommand::Start {
            responder: responder.clone(),
        })?;
        responder.wait()
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        let responder = CommandResponder::new(&self.worker_alive);
        self.send_command(MachineCommand::Shutdown {
            responder: responder.clone(),
        })?;
        responder.wait()
    }

    fn send_command(&self, cmd: MachineCommand) -> anyhow::Result<()> {
        if !self.worker_alive.load(Ordering::Acquire) {
            return Err(anyhow::anyhow!("vm worker already stopped"));
        }
        self.commands.push(cmd);
        Ok(())
    }
}

enum VmMachineState {
    Init(VmInit),
    Running(VmStatusRunning),
    Stopping(VmStatusStopping),
    Stopped,
}

impl VmMachineState {
    fn do_work(&mut self) -> Result<(), RunError> {
        match self {
            VmMachineState::Running(running_vm) => running_vm.do_work()?,
            _ => {
                std::thread::sleep(Duration::from_millis(STATE_POLL_INTERVAL_MS));
            }
        }
        Ok(())
    }
}

/// State machine that owns a VM implementation (`V`) and executes commands in
/// a dedicated worker thread. The public side can enqueue commands and read
/// status without blocking the main control thread.
pub struct VmMachine {
    handle: VmHandle,
    vm: Option<VmMachineState>,
}

impl VmMachine {
    pub(crate) fn new(vm: VmInit) -> anyhow::Result<Self> {
        let handle = VmHandle::new(&vm);
        Ok(Self {
            handle,
            vm: Some(VmMachineState::Init(vm)),
        })
    }

    pub(crate) fn id(&self) -> VmId {
        self.handle.common.id
    }

    pub(crate) fn name(&self) -> &str {
        self.handle.common.name.as_str()
    }

    pub(crate) fn status(&self) -> VMStatus {
        self.handle.state.load()
    }

    pub fn handle(&self) -> VmHandle {
        self.handle.clone()
    }

    fn is_active(&self) -> bool {
        self.status() < VMStatus::Stopping
    }

    pub fn run(&mut self) -> Result<(), RunError> {
        let res = self.run_loop();
        self.handle.state.store(VMStatus::Stopped);
        res
    }

    fn run_loop(&mut self) -> Result<(), RunError> {
        while self.is_active() {
            self.run_loop_once()?;
        }
        Ok(())
    }

    fn run_loop_once(&mut self) -> Result<(), RunError> {
        if let Some(cmd) = self.handle.commands.pop() {
            match cmd {
                MachineCommand::Start { responder } => {
                    let result = match self.vm.take() {
                        Some(VmMachineState::Init(vm_init)) => match vm_init.start() {
                            Ok(running_vm) => {
                                self.vm = Some(VmMachineState::Running(running_vm));
                                self.handle.state.store(VMStatus::Running);
                                Ok(())
                            }
                            Err((e, vm_init)) => {
                                self.vm = Some(VmMachineState::Init(vm_init));
                                Err(e)
                            }
                        },
                        Some(state) => {
                            self.vm = Some(state);
                            Err(anyhow::anyhow!("VM is not in a startable state"))
                        }
                        None => panic!("VM state is missing"),
                    };
                    responder.complete(result);
                }
                MachineCommand::Shutdown { responder } => {
                    let result = match self.vm.take() {
                        Some(VmMachineState::Running(running_vm)) => match running_vm.stop() {
                            Ok(stopping_vm) => {
                                self.vm = Some(VmMachineState::Stopping(stopping_vm));
                                self.handle.state.store(VMStatus::Stopping);
                                Ok(())
                            }
                            Err((e, running_vm)) => {
                                self.vm = Some(VmMachineState::Running(running_vm));
                                Err(e)
                            }
                        },
                        Some(state) => {
                            self.vm = Some(state);
                            Err(anyhow::anyhow!("VM is not in a stoppable state"))
                        }
                        None => panic!("VM state is missing"),
                    };
                    responder.complete(result);
                }
            }
        } else {
            if let Some(vm_state) = &mut self.vm {
                vm_state.do_work()?;
            }
        }

        Ok(())
    }

    // pub(crate) fn start(&self) -> anyhow::Result<()> {}

    // fn worker_loop(
    //     mut vm: V,
    //     state: Arc<AtomicState>,
    //     commands: Arc<CommandMailbox>,
    //     worker_alive: Arc<AtomicBool>,
    // ) {
    //     let mut tracked_state = VMStatus::Loaded;
    //     state.store(tracked_state);
    //     let poll_interval = Duration::from_millis(STATE_POLL_INTERVAL_MS);

    //     loop {
    //         if let Some(cmd) = commands.pop() {
    //             match cmd {
    //                 MachineCommand::Start { responder } => {
    //                     let result = Self::handle_start(&mut vm, &state, &mut tracked_state);
    //                     responder.complete(result);
    //                     Self::sync_state(&vm, &state, &mut tracked_state);
    //                 }
    //                 MachineCommand::Shutdown { responder } => {
    //                     let result = Self::handle_shutdown(&mut vm, &state, &mut tracked_state);
    //                     responder.complete(result);
    //                     Self::sync_state(&vm, &state, &mut tracked_state);
    //                 }
    //                 MachineCommand::Exit => {
    //                     if matches!(tracked_state, VMStatus::Running | VMStatus::Stopping) {
    //                         vm.stop();
    //                     }
    //                     break;
    //                 }
    //             }
    //         } else {
    //             Self::sync_state(&vm, &state, &mut tracked_state);
    //             thread::sleep(poll_interval);
    //         }
    //     }

    //     worker_alive.store(false, Ordering::Release);
    //     state.store(VMStatus::Stopped);
    // }

    // fn handle_start(
    //     vm: &mut V,
    //     state: &Arc<AtomicState>,
    //     tracked_state: &mut VMStatus,
    // ) -> anyhow::Result<()> {
    //     match tracked_state {
    //         VMStatus::Loading => Err(anyhow::anyhow!("VM is still loading")),
    //         VMStatus::Running => Err(anyhow::anyhow!("VM is already running")),
    //         VMStatus::Stopping => Err(anyhow::anyhow!("VM is stopping")),
    //         _ => {
    //             *tracked_state = VMStatus::Running;
    //             state.store(*tracked_state);
    //             if let Err(e) = vm.run() {
    //                 *tracked_state = VMStatus::Stopped;
    //                 state.store(*tracked_state);
    //                 Err(e)
    //             } else {
    //                 Ok(())
    //             }
    //         }
    //     }
    // }

    // fn handle_shutdown(
    //     vm: &mut V,
    //     state: &Arc<AtomicState>,
    //     tracked_state: &mut VMStatus,
    // ) -> anyhow::Result<()> {
    //     match tracked_state {
    //         VMStatus::Loading => Err(anyhow::anyhow!("VM is still loading")),
    //         VMStatus::Stopped => Ok(()),
    //         _ => {
    //             *tracked_state = VMStatus::Stopping;
    //             state.store(*tracked_state);
    //             vm.stop();

    //             loop {
    //                 match vm.status() {
    //                     Status::PoweredOff | Status::Idle => break,
    //                     _ => thread::yield_now(),
    //                 }
    //             }

    //             *tracked_state = VMStatus::Stopped;
    //             state.store(*tracked_state);
    //             Ok(())
    //         }
    //     }
    // }

    // fn sync_state(vm: &V, state: &Arc<AtomicState>, tracked_state: &mut VMStatus) {
    //     let hardware_state = VMStatus::from(vm.status());
    //     if *tracked_state != hardware_state {
    //         *tracked_state = hardware_state;
    //         state.store(hardware_state);
    //     }
    // }
}

/// Auxiliary wrapper that stores the current machine status in an atomically
/// readable form so management threads can query it without synchronisation
/// overhead.
pub(crate) struct AtomicState(AtomicU8);

impl AtomicState {
    pub fn new(state: VMStatus) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    pub fn load(&self) -> VMStatus {
        VMStatus::from_u8(self.0.load(Ordering::Acquire))
    }

    pub fn store(&self, new_state: VMStatus) {
        self.0.store(new_state as u8, Ordering::Release);
    }
}

/// High-level VM lifecycle that is visible to callers of the [`Vm`] API.
/// This is intentionally richer than the low-level `Status` that is returned
/// by the architecture specific implementation so that the shell and
/// management layers can express user-friendly states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VMStatus {
    Loading = 0,
    Loaded = 1,
    Running = 2,
    Suspended = 3,
    Stopping = 4,
    Stopped = 5,
}

impl Default for VMStatus {
    fn default() -> Self {
        VMStatus::Loading
    }
}

impl VMStatus {
    fn from_u8(raw: u8) -> Self {
        match raw {
            0 => VMStatus::Loading,
            1 => VMStatus::Loaded,
            2 => VMStatus::Running,
            3 => VMStatus::Suspended,
            4 => VMStatus::Stopping,
            _ => VMStatus::Stopped,
        }
    }
}

impl From<Status> for VMStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::Idle => VMStatus::Loaded,
            Status::Running => VMStatus::Running,
            Status::ShuttingDown => VMStatus::Stopping,
            Status::PoweredOff => VMStatus::Stopped,
        }
    }
}
