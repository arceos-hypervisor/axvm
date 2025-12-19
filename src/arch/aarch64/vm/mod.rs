use alloc::string::String;

use crate::GuestPhysAddr;

mod init;
mod running;
mod unint;

pub(crate) use init::*;
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
