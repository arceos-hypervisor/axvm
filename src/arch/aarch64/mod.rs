use core::sync::atomic::AtomicBool;

use alloc::string::String;

use crate::{config::AxVMConfig, vm2::*};

pub struct Vm {
    id: VmId,
    name: String,
    set_stop: AtomicBool,
    state: Option<StateMachine>,
}

impl Vm {
    pub fn new(config: AxVMConfig) -> anyhow::Result<Self> {
        let mut s = Self {
            id: config.id().into(),
            name: config.name(),
            set_stop: AtomicBool::new(false),
            state: Some(StateMachine::Idle(config)),
        };
        s.init()?;

        Ok(s)
    }

    fn init(&mut self) -> anyhow::Result<()> {
        let StateMachine::Idle(config) = self.state.take().unwrap() else {
            return Err(anyhow::anyhow!("VM is not in Idle state"));
        };

        self.state = Some(StateMachine::Inited(RunData {}));
        Ok(())
    }

    fn is_active(&self) -> bool {
        !self.set_stop.load(core::sync::atomic::Ordering::SeqCst)
    }
}

impl VmOps for Vm {
    fn id(&self) -> VmId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn boot(&mut self) -> anyhow::Result<()> {
        // self.state = StateMachine::Running;
        Ok(())
    }

    fn stop(&self) {
        self.set_stop
            .store(true, core::sync::atomic::Ordering::SeqCst);
    }

    fn status(&self) -> Status {
        (&(&self.state).unwrap()).into()
    }
}

struct RunData {}

impl RunData {}

enum StateMachine {
    Idle(AxVMConfig),
    Inited(RunData),
    Running(RunData),
    ShuttingDown,
    PoweredOff,
}

impl From<&StateMachine> for Status {
    fn from(value: &StateMachine) -> Self {
        match value {
            StateMachine::Idle(_) => Status::Idle,
            StateMachine::Inited(_) => Status::Idle,
            StateMachine::Running(_) => Status::Running,
            StateMachine::ShuttingDown => Status::ShuttingDown,
            StateMachine::PoweredOff => Status::PoweredOff,
        }
    }
}
