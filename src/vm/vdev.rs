use axvdev::{IrqNum, MmioRegion, VDeviceManager, VirtPlatformOp};

use crate::VmAddrSpace;

#[derive(Clone)]
pub struct VDeviceList {
    inner: VDeviceManager,
    vmspace: VmAddrSpace,
}

impl VDeviceList {
    pub fn new(vmspace: &VmAddrSpace) -> Self {
        let vdev_manager = VDeviceManager::new();
        Self {
            inner: vdev_manager,
            vmspace: vmspace.clone(),
        }
    }
}

impl VirtPlatformOp for VDeviceList {
    fn alloc_mmio_region(
        &self,
        addr: Option<arm_vgic::v3::GuestPhysAddr>,
        size: usize,
        percpu: bool,
    ) -> Option<MmioRegion> {
        todo!()
    }

    fn alloc_irq(&self, irq: Option<IrqNum>) -> Option<IrqNum> {
        todo!()
    }

    fn invoke_irq(&self, irq: IrqNum) {
        todo!()
    }
}
