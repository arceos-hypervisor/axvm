use arm_vgic::v3::VGic;
use axvdev::VDeviceManager;

pub struct PlatData {}

impl PlatData {
    pub fn new(vdev_manager: &VDeviceManager) -> anyhow::Result<Self> {
        Ok(Self {})
    }
}
