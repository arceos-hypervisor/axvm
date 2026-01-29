use alloc::vec::Vec;
use fdt_edit::{Fdt, FdtData, Node, Property, RegInfo, Status};

use crate::{GuestMemory, GuestPhysAddr, vcpu::VCpuCommon, hal::cpu::CpuHardId};

pub(crate) fn fdt_edit() -> Option<Fdt> {
    let addr = axhal::dtb::get_bootarg();
    if addr == 0 {
        return None;
    }
    let fdt = unsafe { Fdt::from_ptr(addr as *mut u8).ok()? };
    Some(fdt)
}

pub fn cpu_list() -> Option<Vec<usize>> {
    let fdt = fdt_edit()?;

    let cpus = fdt
        .find_by_path("/cpus/cpu")
        .filter(|node| node.name().contains("cpu@"))
        .filter(|node| !matches!(node.status(), Some(Status::Disabled)))
        .map(|node| {
            let reg = node
                .regs()
                .unwrap_or_else(|| panic!("cpu {} reg not found", node.name()))[0];
            reg.address as usize
        })
        .collect();
    Some(cpus)
}

pub(crate) struct FdtBuilder {
    fdt: Fdt,
}

impl FdtBuilder {
    pub fn new() -> anyhow::Result<Self> {
        let fdt = fdt_edit().ok_or_else(|| anyhow::anyhow!("No FDT found"))?;
        Ok(Self { fdt })
    }

    pub fn build(self) -> anyhow::Result<FdtData> {
        let dtb_data = self.fdt.encode();
        Ok(dtb_data)
    }

    pub fn setup_cpus<'a>(
        &mut self,
        vcpus: impl Iterator<Item = &'a VCpuCommon>,
    ) -> anyhow::Result<()> {
        let mut rm_nodes = vec![];
        let vcpu_hard_ls = vcpus.map(|v: &VCpuCommon| v.hard_id()).collect::<Vec<_>>();
        for cpu in self.fdt.find_by_path("/cpus/cpu") {
            if let Some(id) = cpu.regs() {
                let id = CpuHardId::new(id[0].address as usize);
                if vcpu_hard_ls.contains(&id) {
                    continue;
                }
            }

            rm_nodes.push(cpu.path());
        }

        for path in rm_nodes {
            self.fdt.remove_node(&path).unwrap();
        }

        Ok(())
    }

    pub fn setup_memory<'a>(
        &mut self,
        memories: impl Iterator<Item = &'a GuestMemory>,
    ) -> anyhow::Result<()> {
        let nodes = self
            .fdt
            .find_by_path("/memory")
            .into_iter()
            .map(|o| o.path())
            .collect::<Vec<_>>();
        for path in nodes {
            self.fdt.remove_node(&path).unwrap();
        }

        for (i, m) in memories.enumerate() {
            let mut node = Node::new(&format!("memory@{i}"));
            let mut prop = Property::new("device_type", vec![]);
            prop.set_string("memory");
            node.add_property(prop);
            self.fdt.root_mut().add_child(node);
            let mut node = self
                .fdt
                .get_by_path_mut(&format!("/memory@{i}"))
                .expect("must has node");
            node.set_regs(&[RegInfo {
                address: m.gpa().as_usize() as u64,
                size: Some(m.size() as u64),
            }]);
        }

        Ok(())
    }

    pub fn setup_chosen(&mut self, initrd: Option<(GuestPhysAddr, usize)>) -> anyhow::Result<()> {
        let mut node = self
            .fdt
            .get_by_path_mut("/chosen")
            .ok_or_else(|| anyhow::anyhow!("No /chosen node found"))?;

        if let Some(initrd) = initrd {
            let cells = node.ctx.parent_address_cells();
            let (initrd_start, initrd_end) = (initrd.0.as_usize(), initrd.0.as_usize() + initrd.1);

            let mut prop_s = Property::new("linux,initrd-start", vec![]);
            let mut prop_e = Property::new("linux,initrd-end", vec![]);

            if cells == 2 {
                prop_s.set_u32_ls(&[initrd_start as u32]);
                prop_e.set_u32_ls(&[initrd_end as u32]);
            } else {
                prop_s.set_u64(initrd_start as _);
                prop_e.set_u64(initrd_end as _);
            }

            node.node.add_property(prop_s);
            node.node.add_property(prop_e);
        } else {
            node.node.remove_property("linux,initrd-start");
            node.node.remove_property("linux,initrd-end");
        };

        if let Some(args) = node.node.get_property_mut("bootargs")
            && let Some(s) = args.as_str()
        {
            let bootargs = s.replace(" ro ", " rw ");
            args.set_string(&bootargs);
        }

        Ok(())
    }
}
