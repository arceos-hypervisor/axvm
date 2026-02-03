use alloc::vec::Vec;
use fdt_edit::{Fdt, FdtData, Node, Property, RegInfo, Status};
use memory_addr::PhysAddr;

use crate::{GuestMemory, GuestPhysAddr, vcpu::VCpuCommon, hal::cpu::CpuHardId};

pub(crate) fn fdt_edit() -> Option<Fdt> {
    let paddr = axhal::dtb::get_bootarg();
    if paddr == 0 {
        return None;
    }
    // Convert physical address to virtual address
    let vaddr = axhal::mem::phys_to_virt(PhysAddr::from(paddr));
    let fdt = unsafe { Fdt::from_ptr(vaddr.as_mut_ptr()).ok()? };
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

        log::info!("[FDT] Removing {} existing memory nodes", nodes.len());
        for path in &nodes {
            log::info!("[FDT]   Removing node: {}", path);
        }
        for path in nodes {
            self.fdt.remove_node(&path).unwrap();
        }

        for (i, m) in memories.enumerate() {
            log::info!(
                "[FDT] Adding memory@{}: GPA={:#x}, size={:#x} ({}MB)",
                i,
                m.gpa().as_usize(),
                m.size(),
                m.size() / (1024 * 1024)
            );
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

        // Set bootargs and stdout-path for guest Linu·x console output
        // const DEFAULT_BOOTARGS: &str = "earlycon=sbi console=hvc0 init=/init root=/dev/vda rw";
        const DEFAULT_BOOTARGS: &str = "earlycon=sbi console=ttyS0,115200 init=/init root=/dev/vda rw";
        const DEFAULT_STDOUT_PATH: &str = "/soc/serial@10000000";

        // Handle bootargs
        if node.node.get_property("bootargs").is_none() {
            let mut prop = Property::new("bootargs", vec![]);
            prop.set_string(DEFAULT_BOOTARGS);
            node.node.add_property(prop);
            log::info!("[FDT] Added bootargs: {}", DEFAULT_BOOTARGS);
        }

        // Handle stdout-path
        if node.node.get_property("stdout-path").is_none() {
            let mut prop = Property::new("stdout-path", vec![]);
            prop.set_string(DEFAULT_STDOUT_PATH);
            node.node.add_property(prop);
            log::info!("[FDT] Added stdout-path: {}", DEFAULT_STDOUT_PATH);
        }

        Ok(())
    }
}
