use arm_gic_driver::IntId;
use arm_vgic::{IrqChipOp, v3};
use axvdev::IrqNum;
use rdif_intc::Intc;

use crate::{fdt::fdt_edit, hal::Ioremap, vdev::VDeviceList};

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
        // self.new_vgic_v3()?;
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

        let config = arm_vgic::VGicConfig::new(crate::hal::cpu::count(), &Ioremap, IrqOp {});

        let mut gic = v3::VGic::new(config);

        self.vdev.add_device(|plat| {
            let gicd = gic.build_gicd(plat.clone(), (gicd.address as usize).into());
            Ok(gicd)
        })?;

        Ok(())
    }
}

fn with_gicv3<F, R>(f: F) -> R
where
    F: FnOnce(&mut arm_gic_driver::v3::Gic) -> R,
{
    let mut g = rdrive::get_one::<Intc>().unwrap().lock().unwrap();
    let gic = g
        .typed_mut::<arm_gic_driver::v3::Gic>()
        .expect("GIC is not GICv3");
    f(gic)
}

fn covnert_irq(irq: IrqNum) -> IntId {
    let id: usize = irq.into();
    unsafe { IntId::raw(id as _) }
}

struct IrqOp {}

impl IrqChipOp for IrqOp {
    fn get_cfg(&self, irq: IrqNum) -> arm_vgic::Trigger {
        let res = with_gicv3(|gic| gic.get_cfg(covnert_irq(irq)));
        match res {
            arm_gic_driver::v3::Trigger::Level => arm_vgic::Trigger::Level,
            arm_gic_driver::v3::Trigger::Edge => arm_vgic::Trigger::Edge,
        }
    }

    fn set_cfg(&self, irq: IrqNum, cfg: arm_vgic::Trigger) {
        let t = match cfg {
            arm_vgic::Trigger::Level => arm_gic_driver::v3::Trigger::Level,
            arm_vgic::Trigger::Edge => arm_gic_driver::v3::Trigger::Edge,
        };
        with_gicv3(|gic| gic.set_cfg(covnert_irq(irq), t));
    }
}
