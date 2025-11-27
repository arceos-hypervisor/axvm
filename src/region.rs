use core::alloc::Layout;

use crate::{
    GuestPhysAddr, HostVirtAddr,
    config::MemoryKind,
    vhal::{phys_to_virt, virt_to_phys},
};

const ALIGN: usize = 1024 * 1024 * 2;

#[derive(Debug, Clone)]
pub struct GuestRegion {
    pub gpa: GuestPhysAddr,
    pub hva: HostVirtAddr,
    pub size: usize,
    pub own: bool,
}

impl GuestRegion {
    pub fn new(kind: &MemoryKind) -> Self {
        match kind {
            MemoryKind::Identical { size } => {
                let hva = HostVirtAddr::from(unsafe {
                    alloc::alloc::alloc(Layout::from_size_align_unchecked(*size, ALIGN))
                } as usize);
                let gpa = GuestPhysAddr::from_usize(virt_to_phys(hva).as_usize());
                GuestRegion {
                    gpa,
                    hva,
                    size: *size,
                    own: true,
                }
            }
            MemoryKind::Passthrough { hpa, size } => {
                let hva = phys_to_virt(*hpa);
                let gpa = GuestPhysAddr::from_usize(hva.as_usize());
                GuestRegion {
                    gpa,
                    hva,
                    size: *size,
                    own: false,
                }
            }
            MemoryKind::Fixed { gpa, size } => {
                let hva = HostVirtAddr::from(unsafe {
                    alloc::alloc::alloc(Layout::from_size_align_unchecked(*size, ALIGN))
                } as usize);
                GuestRegion {
                    gpa: *gpa,
                    hva,
                    size: *size,
                    own: true,
                }
            }
        }
    }

    pub fn buffer_mut(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.hva.as_mut_ptr(), self.size) }
    }
}

impl Drop for GuestRegion {
    fn drop(&mut self) {
        if self.own {
            unsafe {
                alloc::alloc::dealloc(
                    self.hva.as_mut_ptr(),
                    alloc::alloc::Layout::from_size_align(self.size, ALIGN).unwrap(),
                );
            }
        }
    }
}
