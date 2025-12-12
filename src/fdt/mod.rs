use alloc::vec::Vec;
use fdt_edit::{Fdt, Status};

pub(crate) fn fdt_edit() -> Option<Fdt> {
    let addr = axhal::get_bootarg();
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
