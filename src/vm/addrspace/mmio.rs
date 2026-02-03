use core::cell::UnsafeCell;

use alloc::{sync::Arc, vec::Vec};

use crate::{AccessWidth, GuestMemory, GuestPhysAddr};

#[derive(Clone)]
pub struct MmioRegions {
    inner: Arc<UnsafeCell<Inner>>,
}

unsafe impl Send for MmioRegions {}
unsafe impl Sync for MmioRegions {}

struct Inner {
    regions: Vec<GuestMemory>,
}

impl MmioRegions {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(UnsafeCell::new(Inner {
                regions: Vec::new(),
            })),
        }
    }
}

impl MmioRegions {
    pub fn add_region(&mut self, region: GuestMemory) {
        let inner = unsafe { &mut *self.inner.get() };
        inner.regions.push(region);
    }

    pub fn handle_read(&self, addr: GuestPhysAddr, width: AccessWidth) -> Option<usize> {
        let inner = unsafe { &*self.inner.get() };
        for region in &inner.regions {
            if addr >= region.gpa
                && addr.as_usize() + width.size() <= region.gpa.as_usize() + region.size()
            {
                let offset = addr.as_usize() - region.gpa.as_usize();
                let access_ptr = unsafe { region.hva.as_ptr().add(offset) };
                let value = match width {
                    AccessWidth::Byte => unsafe { *(access_ptr as *const u8) as usize },
                    AccessWidth::Word => unsafe { *(access_ptr as *const u16) as usize },
                    AccessWidth::Dword => unsafe { *(access_ptr as *const u32) as usize },
                    AccessWidth::Qword => unsafe { *(access_ptr as *const u64) as usize },
                };
                return Some(value);
            }
        }
        None
    }

    pub fn handle_write(&self, addr: GuestPhysAddr, width: AccessWidth, value: usize) -> bool {
        let inner = unsafe { &*self.inner.get() };
        for region in &inner.regions {
            if addr >= region.gpa
                && addr.as_usize() + width.size() <= region.gpa.as_usize() + region.size()
            {
                let offset = addr.as_usize() - region.gpa.as_usize();
                let access_ptr = unsafe { region.hva.as_ptr().add(offset) };
                match width {
                    AccessWidth::Byte => unsafe { *(access_ptr as *mut u8) = value as u8 },
                    AccessWidth::Word => unsafe { *(access_ptr as *mut u16) = value as u16 },
                    AccessWidth::Dword => unsafe { *(access_ptr as *mut u32) = value as u32 },
                    AccessWidth::Qword => unsafe { *(access_ptr as *mut u64) = value as u64 },
                };
                return true;
            }
        }
        false
    }
}
