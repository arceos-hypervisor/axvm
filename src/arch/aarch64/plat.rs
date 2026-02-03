use arm_vgic::v3;
use axvdev::VDeviceManager;

pub struct PlatData {
    vdev: VDeviceManager,
}

impl PlatData {
    pub fn new(vdev_manager: &VDeviceManager) -> anyhow::Result<Self> {
        Ok(Self {
            vdev: vdev_manager.clone(),
        })
    }
}

fn new_vgic_v3(){
    
}
