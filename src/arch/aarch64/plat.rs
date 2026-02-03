use arm_vgic::v3;

use crate::{fdt::fdt_edit, vdev::VDeviceList};

pub struct PlatData {
    vdev: VDeviceList,
}

impl PlatData {
    pub fn new(vdev_manager: &VDeviceList) -> anyhow::Result<Self> {
        let mut s = Self {
            vdev: vdev_manager.clone(),
        };
        s.init()?;
        Ok(s)
    }

    fn init(&mut self) -> anyhow::Result<()> {
        self.new_vgic_v3()?;
        Ok(())
    }

    fn new_vgic_v3(&mut self) -> anyhow::Result<()> {
        let fdt = fdt_edit().unwrap();
        let mut ls = fdt.find_compatible(&["arm,gic-v3"]);
        if ls.is_empty() {
            return Ok(());
        }

        let node = ls.remove(0);
        debug!("Found GICv3 node: {:?}", node.name());

        let regs = node.regs().ok_or(anyhow!("Gic node has no regs"))?;
        let gicd = regs.get(0).ok_or(anyhow!("No GICD reg"))?;
        let gicr = regs.get(1).ok_or(anyhow!("No GICR reg"))?;
        let plat = self.vdev.new_plat();

        let gic = v3::VGic::new(
            (gicd.address as usize).into(),
            (gicr.address as usize).into(),
            plat.clone(),
        );

        self.vdev.add_device(&plat, gic);

        Ok(())
    }
}
