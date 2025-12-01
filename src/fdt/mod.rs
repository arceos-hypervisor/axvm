use alloc::vec::Vec;
use fdt_parser::{Fdt, Status};

mod r#gen;

pub use r#gen::FdtGen;

pub(crate) fn fdt() -> Option<Fdt> {
    let addr = axhal::get_bootarg();
    if addr == 0 {
        return None;
    }
    let fdt = unsafe { Fdt::from_ptr(addr as *mut u8).ok()? };
    Some(fdt)
}

pub fn cpu_list() -> Option<Vec<usize>> {
    let fdt = fdt()?;

    let nodes = fdt.find_nodes("/cpus/cpu");
    let cpus = nodes
        .into_iter()
        .filter(|node| node.name().contains("cpu@"))
        .filter(|node| !matches!(node.status(), Some(Status::Disabled)))
        .map(|node| {
            let reg = node
                .reg()
                .unwrap_or_else(|_| panic!("cpu {} reg not found", node.name()))[0];
            reg.address as usize
        })
        .collect();
    Some(cpus)
}
