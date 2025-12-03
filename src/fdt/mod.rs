use alloc::vec::Vec;

pub(crate) fn fdt_edit() -> Option<fdt_edit::Fdt> {
    let addr = axhal::get_bootarg();
    if addr == 0 {
        return None;
    }
    let fdt = unsafe { fdt_edit::Fdt::from_ptr(addr as *mut u8).ok()? };
    Some(fdt)
}

pub(crate) fn fdt() -> Option<fdt_parser::Fdt> {
    let addr = axhal::get_bootarg();
    if addr == 0 {
        return None;
    }
    let fdt = unsafe { fdt_parser::Fdt::from_ptr(addr as *mut u8).ok()? };
    Some(fdt)
}

pub fn cpu_list() -> Option<Vec<usize>> {
    let fdt = fdt()?;

    let nodes = fdt.find_nodes("/cpus/cpu");
    let cpus = nodes
        .into_iter()
        .filter(|node| node.name().contains("cpu@"))
        .filter(|node| !matches!(node.status(), Some(fdt_parser::Status::Disabled)))
        .map(|node| {
            let reg = node
                .reg()
                .unwrap_or_else(|_| panic!("cpu {} reg not found", node.name()))[0];
            reg.address as usize
        })
        .collect();
    Some(cpus)
}
