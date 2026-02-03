use arm_vgic::v3;

use crate::vdev::VDeviceList;

pub struct PlatData {
    vdev: VDeviceList,
}

impl PlatData {
    pub fn new(vdev_manager: &VDeviceList) -> anyhow::Result<Self> {
        Ok(Self {
            vdev: vdev_manager.clone(),
        })
    }
}

fn new_vgic_v3() {}
