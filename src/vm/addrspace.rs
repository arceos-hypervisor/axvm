use core::alloc::Layout;

use ranges_ext::RangeInfo;

use crate::GuestPhysAddr;

pub(crate) type AddrSpace = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;
pub(crate) type VmRegionMap = ranges_ext::RangeSetAlloc<VmRegion>;

#[derive(Debug, Clone)]
pub struct VmRegion {
    pub gpa: GuestPhysAddr,
    pub size: usize,
    pub kind: VmRegionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRegionKind {
    Passthrough,
    Memory,
}

impl RangeInfo for VmRegion {
    type Kind = VmRegionKind;

    type Type = GuestPhysAddr;

    fn range(&self) -> core::ops::Range<Self::Type> {
        self.gpa..GuestPhysAddr::from_usize(self.gpa.as_usize() + self.size)
    }

    fn kind(&self) -> &Self::Kind {
        &self.kind
    }

    fn overwritable(&self) -> bool {
        matches!(self.kind, VmRegionKind::Passthrough)
    }

    fn clone_with_range(&self, range: core::ops::Range<Self::Type>) -> Self {
        VmRegion {
            gpa: range.start,
            size: range.end.as_usize() - range.start.as_usize(),
            kind: self.kind,
        }
    }
}
