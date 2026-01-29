use alloc::string::String;

use crate::GuestPhysAddr;

mod inited;
mod running;
mod unint;

pub(crate) use inited::*;
pub(crate) use running::*;
pub(crate) use unint::*;

/// Information about a device in the VM
#[derive(Debug, Clone)]
pub struct DeviceInfo {}

#[derive(Debug, Clone)]
struct DevMapConfig {
    gpa: GuestPhysAddr,
    size: usize,
    name: String,
}
