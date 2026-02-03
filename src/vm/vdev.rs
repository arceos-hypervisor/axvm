use std::{boxed::Box, collections::btree_map::BTreeMap, sync::Arc};

use axvdev::{IrqNum, MmioRegion, VDeviceManager, VirtDeviceOp, VirtPlatformOp};
use spin::{Mutex, RwLock};
use vm_allocator::IdAllocator;

use crate::{AccessWidth, GuestPhysAddr, MmioRegions, VmAddrSpace};

#[derive(Clone)]
pub struct VDevice {
    id: u32,
    raw: Arc<Mutex<Box<dyn VirtDeviceOp>>>,
}

impl VDevice {
    pub fn new(id: u32, raw: impl VirtDeviceOp + 'static) -> Self {
        Self {
            id,
            raw: Arc::new(Mutex::new(Box::new(raw))),
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn run(&self) {
        let mut dev = self.raw.lock();
        dev.run();
    }
}

#[derive(Clone)]
pub struct VDeviceList {
    inner: Arc<RwLock<Inner>>,
    vmspace: VmAddrSpace,
    mmio: MmioRegions,
}

struct Inner {
    id_alloc: IdAllocator,
    deivces: BTreeMap<u32, VDevice>,
}

impl VDeviceList {
    pub fn new(vmspace: &VmAddrSpace) -> Self {
        let mmio = vmspace.mmio_map();
        Self {
            inner: Arc::new(RwLock::new(Inner {
                id_alloc: IdAllocator::new(1, u32::MAX).unwrap(),
                deivces: BTreeMap::new(),
            })),
            vmspace: vmspace.clone(),
            mmio,
        }
    }

    pub fn new_plat(&self) -> VDevPlat {
        VDevPlat {
            id: self.inner.write().id_alloc.allocate_id().unwrap(),
            vdevs: self.clone(),
        }
    }

    fn get_device(&self, id: u32) -> Option<VDevice> {
        let inner = self.inner.read();
        inner.deivces.get(&id).cloned()
    }

    pub fn add_device(&self, plat: &VDevPlat, device: impl VirtDeviceOp + 'static) -> u32 {
        let id = plat.id;
        let mut inner = self.inner.write();
        let vdev = VDevice::new(id, device);
        inner.deivces.insert(id, vdev);
        id
    }

    pub fn handle_mmio_read(&self, addr: GuestPhysAddr, width: AccessWidth) -> Option<usize> {
        let mmio = &self.mmio;
        for (id, region) in unsafe { &(*mmio.inner.get()).regions } {
            if addr >= region.gpa()
                && addr.as_usize() + width.size() <= region.gpa().as_usize() + region.size()
            {
                debug!(
                    "VDev MMIO read addr={:#x} width={:?} dev_id={}",
                    addr.as_usize(),
                    width,
                    id
                );
                self.get_device(*id).unwrap().run();

                let offset = addr.as_usize() - region.gpa().as_usize();
                let access_ptr = unsafe { region.hva().as_ptr().add(offset) };
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
}

#[derive(Clone)]
pub struct VDevPlat {
    id: u32,
    vdevs: VDeviceList,
}

impl VirtPlatformOp for VDevPlat {
    fn alloc_mmio_region(
        &self,
        addr: Option<axvdev::GuestPhysAddr>,
        size: usize,
    ) -> Option<MmioRegion> {
        self.vdevs
            .vmspace
            .new_mmio(
                self.id,
                addr.map(|addr| {
                    let raw: usize = addr.into();
                    raw.into()
                }),
                size,
            )
            .ok()
    }

    fn alloc_irq(&self, irq: Option<IrqNum>) -> Option<IrqNum> {
        todo!()
    }

    fn invoke_irq(&self, irq: IrqNum) {
        todo!()
    }
}
