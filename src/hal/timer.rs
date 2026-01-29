use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::boxed::Box;

use axhal;
use kspin::SpinNoIrq;
use timer_list::{TimeValue, TimerEvent, TimerList};

use crate::hal::percpu::PerCpuSet;

static TOKEN: AtomicUsize = AtomicUsize::new(0);
// const PERIODIC_INTERVAL_NANOS: u64 = axhal::time::NANOS_PER_SEC / axconfig::TICKS_PER_SEC as u64;

/// Represents a timer event in the virtual machine monitor (VMM).
///
/// This struct holds a unique token for the timer and a callback function
/// that will be executed when the timer expires.
pub struct VmmTimerEvent {
    // Unique identifier for the timer event
    token: usize,
    // Callback function to be executed when the timer expires
    timer_callback: Box<dyn FnOnce(TimeValue) + Send + 'static>,
}

impl VmmTimerEvent {
    fn new<F>(token: usize, f: F) -> Self
    where
        F: FnOnce(TimeValue) + Send + 'static,
    {
        Self {
            token,
            timer_callback: Box::new(f),
        }
    }
}

impl TimerEvent for VmmTimerEvent {
    fn callback(self, now: TimeValue) {
        (self.timer_callback)(now)
    }
}

static TIMER_LIST: PerCpuSet<SpinNoIrq<TimerList<VmmTimerEvent>>> = PerCpuSet::new();

/// Registers a new timer that will execute at the specified deadline
///
/// # Arguments
/// - `deadline`: The absolute time in nanoseconds when the timer should trigger
/// - `handler`: The callback function to execute when the timer expires
///
/// # Returns
/// A unique token that can be used to cancel this timer later
pub fn register_timer<F>(deadline: u64, handler: F) -> usize
where
    F: FnOnce(TimeValue) + Send + 'static,
{
    trace!("Registering timer...");
    trace!(
        "deadline is {:#?} = {:#?}",
        deadline,
        TimeValue::from_nanos(deadline)
    );
    let mut timers = TIMER_LIST.lock();
    let token = TOKEN.fetch_add(1, Ordering::Release);
    let event = VmmTimerEvent::new(token, handler);
    timers.set(TimeValue::from_nanos(deadline), event);
    token
}

/// Cancels a timer with the specified token.
///
/// # Parameters
/// - `token`: The unique token of the timer to cancel.
pub fn cancel_timer(token: usize) {
    let mut timers = TIMER_LIST.lock();
    timers.cancel(|event| event.token == token);
}

/// Check and process any pending timer events
pub fn check_events() {
    loop {
        let now = axhal::time::wall_time();
        let event = TIMER_LIST.lock().expire_one(now);
        if let Some((_deadline, event)) = event {
            trace!("pick one {_deadline:#?} to handle!!!");
            event.callback(now);
        } else {
            break;
        }
    }
}

pub fn init() {
    TIMER_LIST.init_with_value(|_| SpinNoIrq::new(TimerList::new()));
}

// /// Schedule the next timer event based on the periodic interval
// pub fn scheduler_next_event() {
//     trace!("Scheduling next event...");
//     let now_ns = axhal::time::monotonic_time_nanos();
//     let deadline = now_ns + PERIODIC_INTERVAL_NANOS;
//     debug!("PHY deadline {} !!!", deadline);
//     axhal::time::set_oneshot_timer(deadline);
// }
