use core::alloc::Layout;

type AddrSpaceRaw = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;

pub struct VmAddrSpace {
    pub(crate) addrspace: AddrSpaceRaw,
}

#[derive(Debug, Clone)]
pub struct VmRegion {
    pub gpa: GuestPhysAddr,
    pub hva: HostVirtAddr,
    pub layout: Layout,
    pub kind: VmRegionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRegionKind {
    Passthrough,
}

impl VmRegion {}
